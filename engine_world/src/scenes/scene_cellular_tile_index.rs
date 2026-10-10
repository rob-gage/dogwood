// Copyright Rob Gage 2026

/// Index of one cellular tile's contiguous cells within the scene's cellular tile buffers
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SceneCellularTileIndex(u32);

impl From<u32> for SceneCellularTileIndex {

    fn from(u32: u32) -> Self { SceneCellularTileIndex(u32) }

}

impl Into<u32> for SceneCellularTileIndex {

    fn into(self) -> u32 { self.0 }

}
