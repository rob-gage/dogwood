// Copyright Rob Gage 2026

/// Index for one retained chunk region
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChunkRegionIndex(u64);

impl From<u64> for ChunkRegionIndex {

    fn from(u64: u64) -> Self { ChunkRegionIndex(u64) }

}

impl Into<u64> for ChunkRegionIndex {

    fn into(self) -> u64 { self.0 }

}