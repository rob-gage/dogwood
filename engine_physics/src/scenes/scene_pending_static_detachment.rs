// Copyright Rob Gage 2026

use crate::tiles::CellCoordinates;

pub(super) struct PendingStaticDetachment {
    pub(super) components: Vec<Vec<CellCoordinates>>,
    pub(super) indices: Vec<u32>,
    pub(super) generation: u64,
    pub(super) ring_offset: (u16, u16),
}
