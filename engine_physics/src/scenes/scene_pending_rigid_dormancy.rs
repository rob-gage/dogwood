// Copyright Rob Gage 2026

use crate::simulation::RigidCellularBodyCell;

pub(super) struct ScenePendingRigidDormancy {
    pub(super) identifier: u64,
    pub(super) position: [f32; 2],
    pub(super) rotation: f32,
    pub(super) linear_velocity: [f32; 2],
    pub(super) angular_velocity: f32,
    pub(super) sleeping: bool,
    pub(super) cells: Vec<RigidCellularBodyCell>,
}
