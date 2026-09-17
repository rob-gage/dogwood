// Copyright Rob Gage 2026

use crate::materials::{CompiledMaterialReaction, MaterialRegistry, MaterialTable};
use crate::simulation_fluids::FluidAuthorityView;
use crate::simulation_materials::material_reactions::MaterialReactions;
use engine_compute::Accelerator;
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
