use super::*;
use crate::{simulation::ScenePhysicsWorld, tiles::CellularAppearance};
use engine_graphics::{Color, MaterialAppearance};
use std::time::{Duration, Instant};

struct Fixture {
    accelerator: Accelerator,
    pressure: CellularPressure,
    materials: MaterialRegistry,
    stone: MaterialIdentifier,
    sand: MaterialIdentifier,
    cells: AcceleratorBuffer,
    kinematics: AcceleratorBuffer,
    occupancy: AcceleratorBuffer,
    velocity: AcceleratorBuffer,
    owners: AcceleratorBuffer,
    rigid_materials: AcceleratorBuffer,
    transforms: AcceleratorBuffer,
    rigid_cells: AcceleratorBuffer,
}

impl Fixture {
    fn new() -> Self {
        let accelerator = Accelerator::new().unwrap();
        let mut materials = MaterialRegistry::new();
        let graphics = MaterialAppearance::from_color(Color::new_rgb(90, 90, 90));
        let stone = materials.register(Material::CellularStatic {
            name: "Stone".into(),
            graphics,
            mass: 1.0,
            pressure_ignore_threshold: 1000.0,
            default_integrity: 100.0,
            debris_material: None,
            debris_yield_rate: 0.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
        });
        let sand = materials.register(Material::CellularDynamic {
            name: "Sand".into(),
            graphics,
            mass: 1.0,
            pressure_transmission: 1.0,
            friction: 0.5,
            restitution: 0.0,
        });
        let cells = accelerator.allocate::<u32>(64);
        let kinematics = accelerator.allocate::<[f32; 4]>(64);
        let occupancy = accelerator.allocate::<u32>(64);
        let velocity = accelerator.allocate::<[f32; 4]>(64);
        let owners = accelerator.allocate::<u32>(64);
        let rigid_materials = accelerator.allocate::<u32>(64);
        let transforms = accelerator.allocate::<[f32; 4]>(3);
        let rigid_cells = accelerator.allocate::<[u32; 8]>(8);
        let integrities = accelerator.allocate::<f32>(64);
        accelerator.wgpu_queue().write_buffer(
            integrities.wgpu_buffer(),
            0,
            &[100.0f32; 64]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        let pressure = CellularPressure::new(
            &accelerator,
            &materials,
            &cells,
            &accelerator.allocate::<u32>(64),
            &integrities,
            &kinematics,
            &occupancy,
            &velocity,
            &owners,
            &rigid_materials,
            &transforms,
            &rigid_cells,
            &accelerator.allocate::<[u32; 4]>(64),
            &accelerator.allocate::<[f32; 2]>(64),
            &accelerator.allocate::<f32>(64),
            &accelerator.allocate::<[f32; 4]>(2),
            &accelerator.allocate::<f32>(64),
            0,
            64,
        );
        Self {
            accelerator,
            pressure,
            materials,
            stone,
            sand,
            cells,
            kinematics,
            occupancy,
            velocity,
            owners,
            rigid_materials,
            transforms,
            rigid_cells,
        }
    }

    fn words(&self, buffer: &AcceleratorBuffer, offset: usize, words: &[u32]) {
        self.accelerator.wgpu_queue().write_buffer(
            buffer.wgpu_buffer(),
            offset as u64 * 4,
            &words
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
    }

    fn floats(&self, buffer: &AcceleratorBuffer, offset: usize, values: &[f32]) {
        self.words(
            buffer,
            offset,
            &values.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        );
    }

    fn tick(&mut self, gravity: [f32; 2], sources: usize) -> RigidGranularReactionBatch {
        self.pressure
            .simulate(
                &self.accelerator,
                TileCoordinates { x: 0, y: 0 },
                1,
                1,
                0,
                0,
                1.0 / 60.0,
                gravity,
                1,
                sources,
                1,
            )
            .unwrap();
        let start = Instant::now();
        loop {
            self.accelerator.poll().unwrap();
            if let Some(batch) = self.pressure.collect_rigid_reactions().unwrap().pop() {
                return batch;
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::yield_now();
        }
    }

    fn static_body(
        &mut self,
        position: [f32; 2],
        angle: f32,
        velocity: [f32; 2],
        width: i32,
    ) -> (ScenePhysicsWorld, crate::simulation::RigidCellularBody) {
        for x in 0..width {
            self.words(
                &self.rigid_cells,
                x as usize * 8,
                &[x as u32, 0, 0, self.stone.as_u32(), 0, 0, 0, 0],
            );
        }
        let mut world = ScenePhysicsWorld::new();
        let body = world.insert_rigid_cellular_body(
            position,
            angle,
            &self.materials,
            (0..width)
                .map(|x| ([x, 0], self.stone, CellularAppearance::NEUTRAL))
                .collect(),
            0.5,
            0.0,
            velocity,
            0.0,
        );
        world.step([0.0; 2], 0.0);
        self.upload(&world, &body);
        (world, body)
    }

    fn upload(&self, world: &ScenePhysicsWorld, body: &crate::simulation::RigidCellularBody) {
        let s = world.rigid_cellular_body_state(body).unwrap();
        self.floats(
            &self.transforms,
            0,
            &[
                s.translation[0],
                s.translation[1],
                s.angle.cos(),
                s.angle.sin(),
                s.linear_velocity[0],
                s.linear_velocity[1],
                s.angular_velocity,
                0.0,
                s.center_of_mass[0],
                s.center_of_mass[1],
                s.inverse_mass,
                s.inverse_angular_inertia,
            ],
        );
    }

    fn read_floats(&self, buffer: &AcceleratorBuffer) -> Vec<f32> {
        let readback = self
            .accelerator
            .wgpu_device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("contact regression readback"),
                size: 64 * 16,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
        let mut encoder = self
            .accelerator
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(buffer.wgpu_buffer(), 0, &readback, 0, 64 * 16);
        self.accelerator.wgpu_queue().submit(Some(encoder.finish()));
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap();
            });
        let start = Instant::now();
        loop {
            self.accelerator.poll().unwrap();
            if let Ok(result) = receiver.try_recv() {
                result.unwrap();
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::yield_now();
        }
        let mapped = readback.slice(..).get_mapped_range().unwrap();
        mapped
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect()
    }

    fn proxy(&self, index: usize) {
        self.words(&self.owners, index, &[1]);
        self.words(&self.rigid_materials, index, &[self.stone.as_u32()]);
        self.words(&self.occupancy, index, &[3]);
    }
}

#[test]
fn static_floor_recovery_sweep_rotation_and_gravity() {
    let _lock = crate::GPU_TEST_LOCK.lock().unwrap();
    let mut f = Fixture::new();
    f.words(&f.cells, 8, &[f.stone.as_u32(); 8]);
    for (angle, gravity) in [(0.0, [0.0, -9.8]), (0.12, [0.0, -9.8]), (0.0, [2.0, -9.8])] {
        let (mut world, body) = f.static_body([0.25, 0.245], angle, [0.0, -0.5], 4);
        for _ in 0..45 {
            world.step(gravity, 1.0 / 60.0);
            f.upload(&world, &body);
            let batch = f.tick(gravity, 4);
            world.apply_rigid_cellular_body_reaction(
                &body,
                [batch.reactions[0][0], batch.reactions[0][1]],
                batch.reactions[0][2],
                batch.energy_budgets[0],
                true,
            );
            let s = world.rigid_cellular_body_state(&body).unwrap();
            assert!(
                s.translation[1] > 0.225,
                "floor penetration: {:?}",
                s.translation
            );
            if angle == 0.0 && gravity[0] == 0.0 {
                assert!(
                    s.angular_velocity.abs() < 0.12,
                    "artificial torque: {}",
                    s.angular_velocity
                );
            }
        }
    }
    let (mut world, body) = f.static_body([0.25, 0.24], 0.0, [0.0; 2], 4);
    let batch = f.tick([0.0; 2], 4);
    assert!(
        batch.energy_budgets[0] > 0.0,
        "no recovery credit: {:?}, contacts {}",
        batch.reactions[0],
        batch.static_contact_counts[0]
    );
    world.apply_rigid_cellular_body_reaction(
        &body,
        [batch.reactions[0][0], batch.reactions[0][1]],
        batch.reactions[0][2],
        batch.energy_budgets[0],
        true,
    );
    assert!(
        world
            .rigid_cellular_body_state(&body)
            .unwrap()
            .linear_velocity[1]
            > 0.01
    );

    f.words(&f.cells, 0, &[0; 64]);
    for y in 0..8 {
        f.words(&f.cells, y * 8 + 3, &[f.stone.as_u32()]);
    }
    let (_world, _body) = f.static_body([0.65, 0.5], 0.0, [30.0, 0.0], 1);
    let batch = f.tick([0.0; 2], 1);
    assert!(
        batch.reactions[0][0] < -1.0,
        "missed swept wall: {:?}",
        batch.reactions[0]
    );
}

#[test]
fn granular_constraints_transfer_and_actor_overlay() {
    let _lock = crate::GPU_TEST_LOCK.lock().unwrap();
    let mut f = Fixture::new();
    // A single bottom face, with the grain's away destination directly below it.
    f.proxy(27);
    f.words(&f.cells, 19, &[f.sand.as_u32()]);
    f.static_body([0.375, 0.375], 0.0, [0.0, -1.0], 1);
    let free = f.tick([0.0, -9.8], 0);
    let free_velocity = f.read_floats(&f.kinematics)[19 * 4 + 1];
    assert!(
        free_velocity < -0.4 && free_velocity > -0.6,
        "free grain: {free_velocity}"
    );
    assert!((free.reactions[0][1] + free_velocity).abs() < 0.02);

    f.words(&f.cells, 11, &[f.sand.as_u32()]);
    f.words(&f.cells, 3, &[f.sand.as_u32()]);
    for downward_speed in [0.0, -0.5, -1.0] {
        f.floats(&f.kinematics, 0, &[0.0; 256]);
        f.floats(&f.transforms, 4, &[0.0, downward_speed, 0.0, 0.0]);
        let packed = f.tick([0.0, -9.8], 0);
        let velocity = f.read_floats(&f.kinematics)[19 * 4 + 1];
        assert!(
            velocity.abs() < 0.001,
            "constraint launched grain: {velocity}"
        );
        assert!(
            packed.reactions[0][1] > -downward_speed + 0.1,
            "packed support: {:?}",
            packed.reactions[0]
        );
    }
    // Feed GPU support through the actual CPU limiter and Rapier repeatedly.
    let (mut world, body) = f.static_body([0.375, 0.375], 0.0, [0.0; 2], 1);
    for _ in 0..40 {
        world.step([0.0, -12.0], 1.0 / 60.0);
        f.upload(&world, &body);
        let batch = f.tick([0.0, -9.8], 0);
        world.apply_rigid_cellular_body_reaction(
            &body,
            [batch.reactions[0][0], batch.reactions[0][1]],
            batch.reactions[0][2],
            batch.energy_budgets[0],
            true,
        );
    }
    assert!(world.rigid_cellular_body_state(&body).unwrap().translation[1] > 0.36);

    // An actor overlay must not replace canonical grain mass or friction.
    f.words(&f.cells, 11, &[0]);
    f.words(&f.cells, 3, &[0]);
    f.words(&f.occupancy, 19, &[1]);
    f.floats(&f.kinematics, 0, &[0.0; 256]);
    f.floats(&f.transforms, 4, &[0.0; 4]);
    f.floats(&f.kinematics, 19 * 4, &[0.2, 1.0, 0.0, 0.0]);
    let overlay = f.tick([0.0; 2], 0);
    let after = f.read_floats(&f.kinematics);
    assert!(
        after[19 * 4 + 1] > 0.45 && after[19 * 4 + 1] < 0.55,
        "overlay changed canonical mass: {}",
        after[19 * 4 + 1]
    );
    assert!(after[19 * 4] < 0.19, "overlay suppressed friction");
    assert!(overlay.granular_contact_counts[0] > 0);
}

#[test]
fn actor_drive_kinematic_constraint_and_canonical_contact() {
    let _lock = crate::GPU_TEST_LOCK.lock().unwrap();
    let mut f = Fixture::new();
    f.proxy(27);
    f.words(&f.occupancy, 26, &[1]);
    let (mut world, body) = f.static_body([0.375, 0.375], 0.0, [0.0; 2], 1);
    for _ in 0..300 {
        world.step([0.0; 2], 1.0 / 60.0);
    }
    f.upload(&world, &body);
    f.floats(&f.velocity, 26 * 4, &[0.0, 0.0, 0.4, 0.0]);
    let drive = f.tick([0.0; 2], 0);
    assert!((drive.reactions[0][0] - 0.4).abs() < 0.01);
    assert!(drive.energy_budgets[0] > 0.0 && drive.moving_contact_counts[0] > 0);
    assert_eq!(drive.granular_contact_counts[0], 0);
    // A sleeping body ignores the same credited reaction until the moving bit wakes it.
    world.apply_rigid_cellular_body_reaction(
        &body,
        [drive.reactions[0][0], 0.0],
        drive.reactions[0][2],
        drive.energy_budgets[0],
        false,
    );
    assert_eq!(
        world
            .rigid_cellular_body_state(&body)
            .unwrap()
            .linear_velocity,
        [0.0; 2]
    );
    world.apply_rigid_cellular_body_reaction(
        &body,
        [drive.reactions[0][0], 0.0],
        drive.reactions[0][2],
        drive.energy_budgets[0],
        drive.moving_contact_counts[0] != 0,
    );
    assert!(
        world
            .rigid_cellular_body_state(&body)
            .unwrap()
            .linear_velocity[0]
            > 0.3
    );
    assert_eq!(
        &f.read_floats(&f.velocity)[26 * 4..26 * 4 + 4],
        &[0.0, 0.0, 0.4, 0.0]
    );

    f.floats(&f.velocity, 26 * 4, &[0.0; 4]);
    f.floats(&f.transforms, 4, &[-1.0, 0.0, 0.0, 0.0]);
    let stationary_actor = f.tick([0.0; 2], 0);
    assert!(stationary_actor.reactions[0][0] > 0.9);
    assert_eq!(stationary_actor.energy_budgets[0], 0.0);

    f.floats(&f.transforms, 4, &[0.0; 4]);
    let idle = f.tick([0.0; 2], 0);
    assert_eq!(idle.reactions[0], [0.0; 3]);
    assert_eq!(idle.energy_budgets[0], 0.0);
    assert_eq!(idle.moving_contact_counts[0], 0);

    f.words(&f.cells, 26, &[f.sand.as_u32()]);
    f.floats(&f.kinematics, 26 * 4, &[1.0, 0.0, 0.0, 0.0]);
    f.floats(&f.velocity, 26 * 4, &[0.0, 0.0, 0.4, 0.0]);
    let both = f.tick([0.0; 2], 0);
    assert!(both.reactions[0][0] > drive.reactions[0][0] + 0.1);
    assert!(both.contact_counts[0] >= 2 && both.granular_contact_counts[0] > 0);

    // Four independent cells each carry a quarter of a unit of actor drive.
    f.words(&f.cells, 0, &[0; 64]);
    f.words(&f.owners, 0, &[0; 64]);
    f.words(&f.occupancy, 0, &[0; 64]);
    f.floats(&f.kinematics, 0, &[0.0; 256]);
    f.floats(&f.velocity, 0, &[0.0; 256]);
    f.static_body([0.25, 0.375], 0.0, [0.0; 2], 4);
    for x in 2..6 {
        f.proxy(24 + x);
        f.words(&f.occupancy, 16 + x, &[2]);
        f.floats(&f.velocity, (16 + x) * 4, &[0.0, 0.0, 0.0, 0.25]);
    }
    let wide = f.tick([0.0; 2], 0);
    assert!(
        (wide.reactions[0][1] - 1.0).abs() < 0.02,
        "drive divided twice: {:?}",
        wide.reactions[0]
    );
    assert_eq!(wide.granular_contact_counts[0], 0);
}
