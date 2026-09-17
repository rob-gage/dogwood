// Copyright Rob Gage 2026

//! Tile-sized authoritative storage and persistence records.

mod chunk;
mod chunk_binary_reader;
mod chunk_entry;
mod chunk_fluid_particle;
mod chunk_gas_cell;
mod chunk_streaming_response;

#[cfg(test)]
pub(crate) mod tests;

pub use chunk::Chunk;
pub(super) use chunk_binary_reader::read_u32;
pub use chunk_entry::ChunkEntry;
pub use chunk_fluid_particle::ChunkFluidParticle;
pub use chunk_gas_cell::ChunkGasCell;
pub use chunk_streaming_response::ChunkStreamingResponse;
