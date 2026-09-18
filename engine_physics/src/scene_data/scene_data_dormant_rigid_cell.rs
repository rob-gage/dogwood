// Copyright Rob Gage 2026

use crate::{materials::MaterialIdentifier, tiles::CellularAppearance};

/// One body-local cellular record retained while a rigid body is dormant
#[derive(Clone)]
pub(crate) struct SceneDormantRigidCell {
    pub(crate) local: [i32; 2],
    pub(crate) material: MaterialIdentifier,
    pub(crate) appearance: CellularAppearance,
    pub(crate) integrity: f32,
    pub(crate) amount: f32,
    pub(crate) temperature: f32,
}
