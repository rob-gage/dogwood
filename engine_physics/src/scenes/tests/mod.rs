// Copyright Rob Gage 2026

mod scene_rigid_body_tests;
mod scene_test_accelerator;
mod scene_test_configuration;
pub(crate) mod scene_test_readback;

use super::{GasDownload, GasUpload};
use crate::chunks::ChunkGasCell;
use crate::materials::{MaterialForm, MaterialIdentifier, MaterialRegistry};
use crate::tiles::{CellCoordinates, TileArea, TileCoordinates};

#[test]
fn test_rejects_a_cell_outside_its_upload_area() {
    let upload: GasUpload = GasUpload::new(
        TileArea::new(TileCoordinates { x: 0, y: 0 }, 1, 1),
        vec![ChunkGasCell {
            coordinates: CellCoordinates { x: 8, y: 0 },
            velocity: [0.0; 2],
            species: vec![(MaterialIdentifier::from_u32(1), 1.0)],
            temperature: 293.15,
        }],
    );
    assert!(upload.validate(&MaterialRegistry::new()).is_err());
}

#[test]
fn test_rejects_invalid_temperature_before_constructing_gas_cells() {
    let area = TileArea::new(TileCoordinates { x: 0, y: 0 }, 1, 1);
    let identifier = MaterialIdentifier::new(MaterialForm::Gas, 0);
    let stride = 5 + 1;
    let mut bytes = vec![0u8; 64 * 64 * stride * 4];
    bytes[16..20].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
    bytes[20..24].copy_from_slice(&1.0f32.to_bits().to_le_bytes());
    assert!(GasDownload::deserialize(&bytes, area, &[identifier]).is_err());
}

use crate::materials::{
    Material, MaterialReaction, MaterialReactionReactant, MaterialReference,
    MaterialRegistryBuilder,
};
use crate::scenes::tests::scene_test_readback::{read_amount, read_cell_state, read_fluid_state};
use crate::scenes::{Scene, SceneData, SceneEditBatch, SceneEditCellPlacement};
use crate::tiles::CellularAppearance;
use engine_graphics::{Color, MaterialAppearance};
use scene_test_accelerator::new_scene_test_accelerator;
use scene_test_configuration::scene_test_configuration;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn test_gas_leaves_and_returns_through_ring_streaming() {
    let (_accelerator_test_lock, accelerator) = new_scene_test_accelerator();
    let mut materials: MaterialRegistry = MaterialRegistry::new();
    let vapor: MaterialIdentifier = materials.register(Material::Gas {
        name: "Vapor".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(120, 160, 190)),
        density: 0.65,
        diffusivity: 0.12,
        extinction: 0.08,
        dissipation: 0.0,
        compressibility: 0.05,
    });
    let mut scene: Scene = Scene::new(
        &accelerator,
        materials,
        scene_test_configuration([0.0, -18.0], 4, 4),
    )
    .unwrap();
    let coordinates: CellCoordinates = CellCoordinates { x: -16, y: 0 };
    let mut edits: SceneEditBatch = SceneEditBatch::new();
    edits.place_material(vapor, CellularAppearance::NEUTRAL, vec![coordinates]);
    scene.test_apply_edits_immediate(&mut edits).unwrap();
    scene.test_shift_to(TileCoordinates { x: 1, y: 0 }).unwrap();
    scene.test_set_origin_target_to_origin();
    let started: Instant = Instant::now();
    while scene.test_has_pending_streaming_downloads() {
        scene.update(Duration::ZERO, false).unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    scene.test_shift_to(TileCoordinates { x: 0, y: 0 }).unwrap();

    let area: TileArea = TileArea::new(TileCoordinates { x: -2, y: 0 }, 1, 1);
    let download: GasDownload =
        GasDownload::new(accelerator.as_ref(), area, 64, scene.test_gas_count());
    scene.test_export_gases(accelerator.as_ref(), &download);
    let byte_count: u64 = 64 * u64::from(5 + scene.test_gas_count()) * 4;
    let (sender, receiver) = mpsc::sync_channel(1);
    download
        .buffer
        .slice(0..byte_count)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap();
        });
    let started: Instant = Instant::now();
    loop {
        accelerator.poll().unwrap();
        if let Ok(result) = receiver.try_recv() {
            result.unwrap();
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    let mapped = download
        .buffer
        .slice(0..byte_count)
        .get_mapped_range()
        .unwrap();
    let bytes: Vec<u8> = mapped.to_vec();
    drop(mapped);
    download.buffer.unmap();
    let restored: Vec<ChunkGasCell> = GasDownload::deserialize(&bytes, area, &[vapor]).unwrap();
    assert!(
        restored
            .iter()
            .any(|cell| cell.coordinates == coordinates && cell.species == vec![(vapor, 1.0)])
    );
}

#[test]
fn test_cellular_indirect_dispatch_executes() {
    let (_accelerator_test_lock, accelerator) = new_scene_test_accelerator();
    let mut materials: MaterialRegistry = MaterialRegistry::new();
    let sand: MaterialIdentifier = materials.register(Material::CellularDynamic {
        name: "Sand".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(194, 178, 128)),
        mass: 1.0,
        pressure_transmission: 0.35,
        friction: 0.65,
        restitution: 0.05,
    });
    let mut scene: Scene = Scene::new(
        &accelerator,
        materials,
        scene_test_configuration([0.0, -18.0], 4, 4),
    )
    .unwrap();
    let mut edits: SceneEditBatch = SceneEditBatch::new();
    edits.place_material(
        sand,
        CellularAppearance::NEUTRAL,
        vec![CellCoordinates { x: 0, y: 8 }],
    );
    scene.test_apply_edits_immediate(&mut edits).unwrap();
    scene.update(Duration::from_secs(1) / 60, true).unwrap();
    accelerator.poll().unwrap();
}

#[test]
fn test_acid_fluid_erodes_same_cell_and_cardinal_stone_across_ticks() {
    let (_accelerator_test_lock, accelerator) = new_scene_test_accelerator();
    let mut materials = MaterialRegistryBuilder::new();
    let stone = materials.register(Material::CellularDynamic {
        name: "Stone".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(100, 100, 100)),
        mass: 1.0,
        pressure_transmission: 1.0,
        friction: 0.5,
        restitution: 0.0,
    });
    let acid = materials.register(Material::Fluid {
        name: "Acid".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(80, 220, 70)),
        pressure_transmission: 1.0,
        friction: 0.0,
        restitution: 0.0,
        rest_density: 1.0,
        artificial_pressure: 0.0,
        xsph_smoothing: 0.0,
        body_push_speed: 0.0,
        density: 1.0,
        viscosity: 1.0,
    });
    materials.tag(stone, "corrodable").unwrap();
    materials.register_reaction(MaterialReaction {
        reactants: [
            Some(MaterialReactionReactant {
                selector: MaterialReference::Material(acid),
                amount: 0.2,
            }),
            Some(MaterialReactionReactant {
                selector: MaterialReference::Tag("corrodable".into()),
                amount: 1.0,
            }),
        ],
        maximum_extent_per_tick: 0.15,
        thermal_energy: 0.01,
        ..Default::default()
    });
    let compiled_materials = materials.compile().unwrap();
    assert_eq!(compiled_materials.reactions().len(), 1);
    assert_eq!(compiled_materials.reaction_selector_members()[1], stone);
    let mut scene = Scene::new(
        &accelerator,
        compiled_materials,
        scene_test_configuration([0.0, 0.0], 4, 4),
    )
    .unwrap();
    let acid_cell = CellCoordinates { x: 0, y: 8 };
    let cardinal_stone_cell = CellCoordinates { x: 1, y: 8 };
    let same_cell = CellCoordinates { x: 2, y: 8 };
    scene.update(Duration::ZERO, false).unwrap();
    let mut edits = SceneEditBatch::new();
    edits.place_material(acid, CellularAppearance::NEUTRAL, vec![acid_cell]);
    edits.place_material(
        stone,
        CellularAppearance::NEUTRAL,
        vec![cardinal_stone_cell, same_cell],
    );
    scene.queue_edits(edits);
    scene.update(Duration::ZERO, false).unwrap();
    scene.update(Duration::ZERO, false).unwrap();
    accelerator.poll().unwrap();
    let cardinal_index = scene.test_cell_edit_index(cardinal_stone_cell).unwrap();
    let same_index = scene.test_cell_edit_index(same_cell).unwrap();
    eprintln!(
        "initial: cardinal={:?} same={:?}",
        read_cell_state(accelerator.as_ref(), &scene, cardinal_index),
        read_cell_state(accelerator.as_ref(), &scene, same_index)
    );
    let mut previous = 1.0;
    let mut sequence = Vec::new();
    for tick in 1..=7 {
        scene.update(Duration::from_secs(1) / 60, true).unwrap();
        let (_, cardinal_amount) = read_cell_state(accelerator.as_ref(), &scene, cardinal_index);
        assert!(cardinal_amount <= previous + 0.00001);
        assert!((cardinal_amount - (1.0 - tick as f32 * 0.15).max(0.0)).abs() < 0.0001);
        sequence.push(cardinal_amount);
        previous = cardinal_amount;
    }
    eprintln!("multi-tick Acid erosion: {sequence:?}");

    scene.test_commit_reserved_particle(
        accelerator.as_ref(),
        scene.test_particle_capacity() - 1,
        acid.as_u32(),
        [same_cell.x as f32 + 0.5, same_cell.y as f32 + 0.5].map(|coordinate| coordinate / 8.0),
        [0.0, 0.0],
        0.8,
        293.15,
    );
    accelerator.wgpu_queue().write_buffer(
        scene
            .test_cellular_material_identifiers_buffer()
            .wgpu_buffer(),
        same_index as u64 * 4,
        &stone.as_u32().to_le_bytes(),
    );
    accelerator.wgpu_queue().write_buffer(
        scene.test_cellular_amounts_buffer().wgpu_buffer(),
        same_index as u64 * 4,
        &1.0f32.to_le_bytes(),
    );
    accelerator.poll().unwrap();
    let mut same_sequence = Vec::new();
    for tick in 1..=7 {
        scene.update(Duration::from_secs(1) / 60, true).unwrap();
        let (same_material, same_amount) =
            read_cell_state(accelerator.as_ref(), &scene, same_index);
        let (acid_material, acid_active, acid_amount) = read_fluid_state(
            accelerator.as_ref(),
            &scene,
            scene.test_particle_capacity() - 1,
        );
        assert_eq!(acid_material, acid.as_u32());
        assert_eq!(acid_active, 1);
        assert!(acid_amount > 0.000001);
        assert!((same_amount - (1.0 - tick as f32 * 0.15).max(0.0)).abs() < 0.0001);
        if tick < 7 {
            assert_eq!(same_material, stone.as_u32());
        } else {
            assert_eq!(same_material, MaterialIdentifier::NULL.as_u32());
        }
        same_sequence.push(same_amount);
    }
    eprintln!("same-cell Acid erosion: {same_sequence:?}");
}

#[test]
fn test_acid_fluid_erodes_rigid_stone_and_removes_topology() {
    let (_accelerator_test_lock, accelerator) = new_scene_test_accelerator();
    let mut materials = MaterialRegistryBuilder::new();
    let stone = materials.register(Material::CellularStatic {
        name: "Stone".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(100, 100, 100)),
        mass: 1.0,
        pressure_ignore_threshold: 1.0,
        default_integrity: 1.0,
        minimum_rigid_body_cell_count: 1,
        debris_material: None,
        debris_yield_rate: 0.0,
        pressure_transmission: 1.0,
        friction: 0.5,
        restitution: 0.0,
    });
    let acid = materials.register(Material::Fluid {
        name: "Acid".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(80, 220, 70)),
        pressure_transmission: 1.0,
        friction: 0.0,
        restitution: 0.0,
        rest_density: 1.0,
        artificial_pressure: 0.0,
        xsph_smoothing: 0.0,
        body_push_speed: 0.0,
        density: 1.0,
        viscosity: 1.0,
    });
    materials.tag(stone, "corrodable").unwrap();
    materials.register_reaction(MaterialReaction {
        reactants: [
            Some(MaterialReactionReactant {
                selector: MaterialReference::Material(acid),
                amount: 0.2,
            }),
            Some(MaterialReactionReactant {
                selector: MaterialReference::Tag("corrodable".into()),
                amount: 1.0,
            }),
        ],
        maximum_extent_per_tick: 0.15,
        thermal_energy: 0.01,
        ..Default::default()
    });
    let mut scene = Scene::new(
        &accelerator,
        materials.compile().unwrap(),
        scene_test_configuration([0.0, 0.0], 4, 4),
    )
    .unwrap();
    scene.update(Duration::ZERO, false).unwrap();
    let acid_cell = CellCoordinates { x: 0, y: 8 };
    let stone_cell = CellCoordinates { x: 1, y: 8 };
    let mut edits = SceneEditBatch::new();
    edits.place_material(acid, CellularAppearance::NEUTRAL, vec![acid_cell]);
    edits.place_rigid_body(vec![SceneEditCellPlacement {
        coordinates: stone_cell,
        material_identifier: stone,
        appearance: CellularAppearance::NEUTRAL,
    }]);
    scene.queue_edits(edits);
    scene.update(Duration::ZERO, false).unwrap();
    scene.update(Duration::ZERO, false).unwrap();
    assert_eq!(scene.test_rigid_cellular_bodies().len(), 1);
    let state_slot = scene.test_rigid_cellular_bodies()[0].cells[0].state_slot;
    let mut sequence = Vec::new();
    for _ in 1..=7 {
        scene.update(Duration::from_secs(1) / 60, true).unwrap();
        sequence.push(read_amount(
            accelerator.as_ref(),
            scene.rigid_cell_amounts_buffer(),
            state_slot,
        ));
    }
    assert!(sequence.windows(2).all(|pair| pair[1] <= pair[0] + 0.00001));
    for (tick, amount) in sequence.iter().enumerate() {
        assert!((*amount - (1.0 - (tick + 1) as f32 * 0.15).max(0.0)).abs() < 0.0001);
    }
    for _ in 0..3 {
        scene.update(Duration::ZERO, false).unwrap();
    }
    assert!(scene.test_rigid_cellular_bodies().is_empty());
    eprintln!("rigid Acid erosion: {sequence:?}");
}

#[test]
fn test_full_screen_moving_sand_headless_tps() {
    let (_accelerator_test_lock, accelerator) = new_scene_test_accelerator();
    let mut materials = MaterialRegistry::new();
    let sand = materials.register(Material::CellularDynamic {
        name: "Sand".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(194, 178, 128)),
        mass: 1.0,
        pressure_transmission: 0.35,
        friction: 0.65,
        restitution: 0.05,
    });
    let data_path = std::env::temp_dir().join(format!(
        "dogwood-sand-stress-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&data_path).unwrap();
    materials
        .serialize(&mut std::fs::File::create(data_path.join("materials")).unwrap())
        .unwrap();
    let data = SceneData::load(data_path.clone()).unwrap();
    let mut scene = Scene::load(
        &accelerator,
        scene_test_configuration([0.0, -18.0], 8, 8),
        data,
    )
    .unwrap();
    let mut edits = SceneEditBatch::new();
    edits.place_material(
        sand,
        CellularAppearance::NEUTRAL,
        (8..64)
            .step_by(2)
            .flat_map(|y| (0..64).map(move |x| CellCoordinates { x, y }))
            .collect(),
    );
    scene.test_apply_edits_immediate(&mut edits).unwrap();
    let tick = Duration::from_secs(1) / 60;
    for _ in 0..5 {
        scene.update(tick, true).unwrap();
    }
    let start = Instant::now();
    let mut older_snapshots = 0;
    let mut maximum_snapshot_age = 0;
    for _ in 0..30 {
        scene.update(tick, true).unwrap();
        let age = scene
            .test_physics_world()
            .terrain_bridge_statistics()
            .collision_snapshot_age;
        maximum_snapshot_age = maximum_snapshot_age.max(age);
        older_snapshots += u32::from(age > 1);
    }
    let elapsed = start.elapsed();
    let stats = scene.test_physics_world().terrain_bridge_statistics();
    assert_eq!(stats.dynamic_shape_rebuilds, 0);
    assert_eq!(stats.dynamic_cells_scanned, 0);
    eprintln!(
        "moving sand headless TPS: {:.1}, snapshot age max {}, ticks >1 {}",
        30.0 / elapsed.as_secs_f64(),
        maximum_snapshot_age,
        older_snapshots
    );
    drop(scene);
    std::fs::remove_dir_all(data_path).unwrap();
}
