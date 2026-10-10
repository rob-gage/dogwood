// Copyright Rob Gage 2026

/// Index for one retained chunk region
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SceneChunkRegionIndex(u64);

impl From<u64> for SceneChunkRegionIndex {

    fn from(u64: u64) -> Self { SceneChunkRegionIndex(u64) }

}

impl Into<u64> for SceneChunkRegionIndex {

    fn into(self) -> u64 { self.0 }

}