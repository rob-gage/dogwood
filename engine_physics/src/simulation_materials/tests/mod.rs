// Copyright Rob Gage 2026

mod material_mutation_test_readback;
mod reaction_candidate;
mod reaction_environment;
mod reaction_extent;
mod reaction_implicit_air;
mod reaction_rule_tests;

use super::MaterialMutations;
use crate::materials::{Material, MaterialIdentifier, MaterialRegistry, MaterialTable};
use crate::simulation::Fluids;
use crate::simulation::tests::new_accelerator_test;
use crate::simulation_fluids::FluidAuthorityView;
use crate::simulation_materials::material_reactions::MaterialReactions;
use engine_graphics::{Color, MaterialAppearance};
use material_mutation_test_readback::read_u32;

#[test]
fn test_resolver_pipeline_compiles() {
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
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
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
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
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
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
        // pressure resolution must not consume the previous thermal candidate buffer.
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

#[test]
fn test_apply_shader_is_commit_only() {
    let shader = concat!(
        include_str!("../material_reactions_shader_helpers.wgsl"),
        include_str!("../material_reactions_shader_fluid_reservation.wgsl"),
        include_str!("../material_reactions_shader_reservation.wgsl"),
        include_str!("../material_reactions_shader_candidate_reservation.wgsl"),
        include_str!("../material_reactions_shader_fluid_authority.wgsl"),
        include_str!("../material_reactions_shader_application.wgsl"),
    );
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
    let (_accelerator_test_lock, accelerator) = new_accelerator_test();
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
