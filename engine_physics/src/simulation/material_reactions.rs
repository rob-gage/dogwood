// Copyright Rob Gage 2026

//! Shared deterministic reaction eligibility and arbitration rules. GPU discovery
//! emits the same compact candidate shape; keeping arbitration here makes the
//! ordering contract explicit and independently testable.

use super::ReactionMaterialTable;
use crate::materials::CompiledMaterialReaction;
use engine_compute::{Accelerator, AcceleratorBuffer};
use std::collections::BTreeSet;

/// Immutable-snapshot GPU reaction discovery. Application is intentionally a
/// separate stage so no product becomes an input until the next chemistry tick.
pub(crate) struct MaterialReactions {
    candidates: AcceleratorBuffer,
    reaction_energy: AcceleratorBuffer,
    parameters: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    discover_pipeline: wgpu::ComputePipeline,
    apply_pipeline: wgpu::ComputePipeline,
    cell_count: u32,
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
        retained_pressure: &AcceleratorBuffer,
        fluid_coverage: &AcceleratorBuffer,
        gas_concentrations: &AcceleratorBuffer,
        external_occupancy: &AcceleratorBuffer,
        rigid_claims: &AcceleratorBuffer,
        reaction_energy: AcceleratorBuffer,
        pending_pressure: &AcceleratorBuffer,
        mutation_requests: &AcceleratorBuffer,
        mutation_request_count: &AcceleratorBuffer,
        cell_count: u32,
        gas_count: u32,
        reaction_count: u32,
    ) -> Self {
        let device = accelerator.wgpu_device();
        let candidates = accelerator.allocate::<[u32; 4]>(cell_count as usize);
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
                storage(3, true),
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
        Self {
            candidates,
            reaction_energy,
            parameters,
            bind_group,
            discover_pipeline: pipeline(
                "discover_canonical",
                "material reaction discovery pipeline",
            ),
            apply_pipeline: pipeline(
                "apply_canonical",
                "material reaction canonical apply pipeline",
            ),
            cell_count,
            reaction_count,
        }
    }
    pub(crate) fn encode(&self, accelerator: &Accelerator, encoder: &mut wgpu::CommandEncoder) {
        if self.reaction_count == 0 {
            return;
        }
        let mut pass = accelerator.begin_compute_pass(encoder, "chemistry discover canonical");
        pass.set_pipeline(&self.discover_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&self.apply_pipeline);
        pass.dispatch_workgroups(self.cell_count.div_ceil(64), 1, 1);
    }
    pub(crate) const fn candidates_buffer(&self) -> &AcceleratorBuffer {
        &self.candidates
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
    candidates.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.authoring_order.cmp(&b.authoring_order))
            .then_with(|| a.anchor.cmp(&b.anchor))
            .then_with(|| a.reaction_index.cmp(&b.reaction_index))
    });
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
    fn extent_is_stoichiometric() {
        assert_eq!(extent(0.5, [1.0, 0.4], [1.0, 2.0]), 0.2);
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
    fn canonical_discovery_and_apply_pipelines_compile() {
        let accelerator = Accelerator::new().unwrap();
        let registry = MaterialRegistry::new();
        let table = ReactionMaterialTable::new(&accelerator, &registry);
        let ids = accelerator.allocate::<u32>(64);
        let amounts = accelerator.allocate::<f32>(64);
        let temperatures = accelerator.allocate::<f32>(64);
        let pressure = accelerator.allocate::<[f32; 4]>(64);
        let coverage = accelerator.allocate::<f32>(64);
        let gas = accelerator.allocate::<f32>(1);
        let occupancy = accelerator.allocate::<u32>(64);
        let claims = accelerator.allocate::<u32>(64);
        let requests = accelerator.allocate::<[u32; 9]>(128);
        let request_count = accelerator.allocate::<u32>(1);
        let reaction_energy = accelerator.allocate::<f32>(64);
        let pending_pressure = accelerator.allocate::<[f32; 4]>(64);
        let _reactions = MaterialReactions::new(
            &accelerator,
            &table,
            &ids,
            &amounts,
            &temperatures,
            &pressure,
            &coverage,
            &gas,
            &occupancy,
            &claims,
            reaction_energy,
            &pending_pressure,
            &requests,
            &request_count,
            64,
            0,
            0,
        );
    }
}
