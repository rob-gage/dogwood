// Copyright Rob Gage 2026

mod material_mutation_test_readback;

use super::MaterialMutations;
use crate::materials::{
    CompiledMaterialReaction, Material, MaterialIdentifier, MaterialRegistry, MaterialTable,
};
use crate::simulation::Fluids;
use crate::simulation_fluids::FluidAuthorityView;
use crate::simulation_materials::material_reactions::MaterialReactions;
use engine_compute::Accelerator;
use engine_graphics::{Color, MaterialAppearance};
use material_mutation_test_readback::read_u32;

#[test]
fn test_resolver_pipeline_compiles() {
    let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
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
fn test_indirect_resolver_replaces_and_deletes_cells() {
    let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
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
fn test_gas_condensation_aggregates_a_tile_into_unit_particles() {
    let _accelerator_test_lock = crate::simulation::tests::acquire_accelerator_test_lock();
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
        // Pressure resolution must not consume the previous thermal candidate buffer.
        mutations.resolve_requests(&accelerator, 64, 1);
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
        let mut encoder =
            accelerator
                .wgpu_device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("test thermal condensation"),
                });
        mutations.encode_thermal_condensation(&accelerator, &mut encoder, 64, 1);
        accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let particle_words = read_u32(&accelerator, &particles, 40);
        assert_eq!(
            particle_words
                .as_chunks::<10>()
                .0
                .iter()
                .filter(|particle| particle[1] != 0)
                .count(),
            tick
        );
        assert!(
            particle_words
                .as_chunks::<10>()
                .0
                .iter()
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
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ReactionEnvironment {
    pub temperature: f32,
    pub pressure: f32,
    pub air: f32,
}

fn environment_matches(rule: &CompiledMaterialReaction, env: ReactionEnvironment) -> bool {
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
fn implicit_air(
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
struct ReactionCandidate {
    pub anchor: u32,
    pub reaction_index: u32,
    pub priority: i32,
    pub authoring_order: u32,
    pub extent: f32,
    pub authorities: [Option<u64>; 2],
}

/// Sort by explicit priority, stable authoring order, then anchor. Accepted
/// candidates reserve every authority as an all-or-nothing set, so neither Accelerator
/// invocation order nor overlapping raster claims can double-consume matter.
fn resolve_contention(mut candidates: Vec<ReactionCandidate>) -> Vec<ReactionCandidate> {
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
fn extent(
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

#[test]
fn test_environment_bounds_are_independent() {
    let rule = CompiledMaterialReaction {
        minimum_temperature: 10.0,
        maximum_temperature: 20.0,
        minimum_pressure: 2.0,
        maximum_pressure: 4.0,
        minimum_air: 0.25,
        maximum_air: 0.75,
        ..Default::default()
    };
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
fn test_contention_is_priority_then_stable_and_atomic() {
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
fn test_contention_tie_uses_authoring_order() {
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
fn test_contention_final_tie_uses_reaction_index() {
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
fn test_extent_is_stoichiometric() {
    assert_eq!(extent(0.5, [1.0, 0.4], [1.0, 2.0]), 0.2);
    // Two slots resolving to one authority share its inventory budget.
    assert_eq!(extent(1.0, [0.7], [1.0 + 1.0]), 0.35);
}
#[test]
fn test_implicit_air_respects_local_occupancy() {
    assert_eq!(implicit_air(true, false, 0.0, 0.0), 1.0);
    assert_eq!(implicit_air(true, false, 0.0, 0.25), 0.75);
    assert_eq!(implicit_air(true, false, 0.0, 1.0), 0.0);
    assert_eq!(implicit_air(false, false, 0.0, 0.0), 0.0);
    assert_eq!(implicit_air(true, true, 0.0, 0.0), 0.0);
}

#[test]
fn test_apply_shader_is_commit_only() {
    let shader = include_str!("../material_reactions.wgsl");
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
fn test_canonical_discovery_and_apply_pipelines_compile() {
    let accelerator = Accelerator::new().unwrap();
    let registry = MaterialRegistry::new();
    let table = MaterialTable::new(&accelerator, &registry);
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
