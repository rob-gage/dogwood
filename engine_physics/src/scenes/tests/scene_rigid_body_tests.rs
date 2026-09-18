// Copyright Rob Gage 2026

use super::scene_test_accelerator::new_scene_test_accelerator;
use super::scene_test_configuration::scene_test_configuration;
use super::scene_test_readback::read_cell_state;
use crate::materials::{
    Material, MaterialIdentifier, MaterialRegistry, MaterialRegistryBuilder,
    MaterialThermalProperties, MaterialThermalTransition,
};
use crate::scenes::{Scene, SceneEditBatch, SceneEditCellPlacement};
use crate::simulation::CollisionOccupancySnapshot;
use crate::tiles::{CellCoordinates, CellularAppearance, TileCoordinates};
use engine_graphics::{Color, MaterialAppearance};
use std::time::Duration;

#[test]
fn test_disconnected_static_component_becomes_one_falling_rigid_body() {
    let (_accelerator_test_lock, accelerator) = new_scene_test_accelerator();
    let mut materials: MaterialRegistry = MaterialRegistry::new();
    let stone: MaterialIdentifier = materials.register(Material::CellularStatic {
        name: "Stone".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(110, 105, 100)),
        mass: 1.0,
        pressure_ignore_threshold: 1.0,
        default_integrity: 1.0,
        minimum_rigid_body_cell_count: 4,
        debris_material: None,
        debris_yield_rate: 0.0,
        pressure_transmission: 0.5,
        friction: 0.7,
        restitution: 0.05,
    });
    let mut scene: Scene = Scene::new(
        &accelerator,
        materials,
        scene_test_configuration([0.0, -8.0], 1, 1),
    )
    .unwrap();
    let cells: Vec<CellCoordinates> = (2..=5)
        .flat_map(|x| (3..=4).map(move |y| CellCoordinates { x, y }))
        .chain((0..=2).map(|y| CellCoordinates { x: 3, y }))
        .collect();
    let mut edits = SceneEditBatch::new();
    edits.place_material(stone, CellularAppearance::NEUTRAL, cells.clone());
    scene.test_apply_edits_immediate(&mut edits).unwrap();
    let mut static_masks = vec![[0u32; 2]; 25];
    for cell in &cells {
        let tile_x = cell.x.div_euclid(8) + 2;
        let tile_y = cell.y.div_euclid(8) + 2;
        let tile = (tile_y * 5 + tile_x) as usize;
        let local = (cell.y.rem_euclid(8) * 8 + cell.x.rem_euclid(8)) as usize;
        static_masks[tile][local / 32] |= 1 << (local % 32);
    }
    let baseline = CollisionOccupancySnapshot {
        sequence: 0,
        origin: TileCoordinates { x: -2, y: -2 },
        width: 5,
        height: 5,
        static_masks: static_masks.into_boxed_slice(),
        dynamic_masks: vec![[0, 0]; 25].into_boxed_slice(),
    };
    scene
        .test_detach_unanchored_static_components(&mut baseline.clone())
        .unwrap();
    assert!(scene.test_rigid_cellular_bodies().is_empty());
    let mut separated = baseline.clone();
    separated.sequence = 1;
    separated.clear_static_cell(3, 2);
    scene
        .test_detach_unanchored_static_components(&mut separated)
        .unwrap();
    for _ in 0..20 {
        accelerator.poll().unwrap();
        scene.test_apply_completed_static_detachment().unwrap();
        if scene.test_rigid_cellular_bodies().len() == 1 {
            break;
        }
        std::thread::yield_now();
    }
    assert!(scene.test_rigid_cellular_bodies().len() == 1);
    assert!(scene.test_rigid_cellular_bodies()[0].cells.len() == 8);
    let initial_y = scene.test_rigid_cellular_body_state().unwrap().translation[1];
    scene
        .test_physics_world()
        .update_cellular_snapshot(separated);
    let gravity = scene.test_gravity();
    for _ in 0..8 {
        scene.test_physics_world().step(gravity, 1.0 / 60.0);
    }
    let state = scene.test_rigid_cellular_body_state().unwrap();
    assert!(state.translation[1] < initial_y);
    scene.test_rasterize_rigid_cellular_bodies(accelerator.as_ref(), state);
    accelerator.poll().unwrap();
}

#[test]
fn test_accelerator_phase_static_cells_resolve_to_debris_or_rigid_body() {
    let (_accelerator_test_lock, accelerator) = new_scene_test_accelerator();
    let mut builder = MaterialRegistryBuilder::new();
    let debris = builder.register(Material::CellularDynamic {
        name: "Debris".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(150, 120, 90)),
        mass: 1.0,
        pressure_transmission: 1.0,
        friction: 0.5,
        restitution: 0.0,
    });
    let stone = builder.register(Material::CellularStatic {
        name: "Frozen Stone".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(110, 105, 100)),
        mass: 1.0,
        pressure_ignore_threshold: 1.0,
        default_integrity: 1.0,
        minimum_rigid_body_cell_count: 3,
        debris_material: Some(debris),
        debris_yield_rate: 1.0,
        pressure_transmission: 0.5,
        friction: 0.7,
        restitution: 0.05,
    });
    let fluid = builder.register(Material::Fluid {
        name: "Freezing Fluid".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(90, 180, 230)),
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
    builder
        .set_thermal(
            debris,
            MaterialThermalProperties {
                conductivity: 0.1,
                specific_heat_capacity: 1.0,
                default_temperature: Some(293.15),
                ..Default::default()
            },
        )
        .unwrap();
    builder
        .set_thermal(
            stone,
            MaterialThermalProperties {
                conductivity: 0.1,
                specific_heat_capacity: 1.0,
                default_temperature: Some(293.15),
                ..Default::default()
            },
        )
        .unwrap();
    builder
        .set_thermal(
            fluid,
            MaterialThermalProperties {
                conductivity: 0.1,
                specific_heat_capacity: 1.0,
                default_temperature: Some(400.0),
                hot_transition: Some(MaterialThermalTransition {
                    threshold_temperature: 300.0,
                    target: stone,
                    yield_rate: 1.0,
                    latent_energy: 0.0,
                }),
                ..Default::default()
            },
        )
        .unwrap();
    let mut scene = Scene::new(
        &accelerator,
        builder.compile().unwrap(),
        scene_test_configuration([0.0, 0.0], 4, 4),
    )
    .unwrap();
    scene.update(Duration::from_secs(1) / 60, true).unwrap();
    let isolated = CellCoordinates { x: 8, y: 8 };
    let rigid_cells = [
        CellCoordinates { x: 16, y: 8 },
        CellCoordinates { x: 17, y: 8 },
        CellCoordinates { x: 16, y: 9 },
    ];
    let mut edits = SceneEditBatch::new();
    edits.place_material(
        fluid,
        CellularAppearance::NEUTRAL,
        std::iter::once(isolated)
            .chain(rigid_cells.iter().copied())
            .collect(),
    );
    scene.queue_edits(edits);
    scene.update(Duration::ZERO, false).unwrap();
    scene.update(Duration::ZERO, false).unwrap();
    for _ in 0..40 {
        scene.update(Duration::from_secs(1) / 60, true).unwrap();
        scene.update(Duration::ZERO, false).unwrap();
        if scene.test_rigid_cellular_bodies().len() == 1
            && read_cell_state(
                accelerator.as_ref(),
                &scene,
                scene.test_cell_edit_index(isolated).unwrap(),
            )
            .0 == debris.as_u32()
        {
            break;
        }
    }
    let isolated_state = read_cell_state(
        accelerator.as_ref(),
        &scene,
        scene.test_cell_edit_index(isolated).unwrap(),
    );
    assert_eq!(isolated_state.0, debris.as_u32());
    assert_eq!(scene.test_rigid_cellular_bodies().len(), 1);
    assert_eq!(scene.test_rigid_cellular_bodies()[0].cells.len(), 3);
    assert!(
        scene.test_rigid_cellular_bodies()[0].cells.iter().all(
            |cell| cell.material == stone && cell.appearance.0 == CellularAppearance::NEUTRAL.0
        )
    );
}

#[test]
fn test_queued_authored_rigid_body_is_atomic_and_body_local() {
    let (_accelerator_test_lock, accelerator) = new_scene_test_accelerator();
    let mut materials = MaterialRegistry::new();
    let stone = materials.register(Material::CellularStatic {
        name: "Stone".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
        mass: 2.0,
        pressure_ignore_threshold: 1.0,
        default_integrity: 1.0,
        minimum_rigid_body_cell_count: 1,
        debris_material: None,
        debris_yield_rate: 0.0,
        pressure_transmission: 1.0,
        friction: 0.7,
        restitution: 0.05,
    });
    let sand = materials.register(Material::CellularDynamic {
        name: "Sand".into(),
        graphics: MaterialAppearance::from_color(Color::new_rgb(1, 1, 1)),
        mass: 1.0,
        pressure_transmission: 1.0,
        friction: 0.5,
        restitution: 0.0,
    });
    let mut scene = Scene::new(
        &accelerator,
        materials,
        scene_test_configuration([0.0, -8.0], 4, 4),
    )
    .unwrap();
    let mut edits = SceneEditBatch::new();
    edits.place_rigid_body(vec![
        SceneEditCellPlacement {
            coordinates: CellCoordinates { x: 10, y: 20 },
            material_identifier: stone,
            appearance: CellularAppearance(3),
        },
        SceneEditCellPlacement {
            coordinates: CellCoordinates { x: 11, y: 20 },
            material_identifier: stone,
            appearance: CellularAppearance(4),
        },
        SceneEditCellPlacement {
            coordinates: CellCoordinates { x: 10, y: 20 },
            material_identifier: stone,
            appearance: CellularAppearance(5),
        },
    ]);
    edits.place_rigid_body(vec![SceneEditCellPlacement {
        coordinates: CellCoordinates { x: 12, y: 20 },
        material_identifier: sand,
        appearance: CellularAppearance::NEUTRAL,
    }]);
    scene.queue_edits(edits);
    scene.update(Duration::ZERO, false).unwrap();
    assert_eq!(scene.test_rigid_cellular_bodies().len(), 1);
    let body = &scene.test_rigid_cellular_bodies()[0];
    assert_eq!(body.cells.len(), 2);
    assert_eq!(body.cells[0].local, [0, 0]);
    assert_eq!(body.cells[1].local, [1, 0]);
    assert_eq!(body.cells[0].appearance.0, 5);
    assert_eq!(scene.test_rigid_cellular_topology_revision(), 1);
}
