// Copyright Rob Gage 2026

use std::io;

/// An I/O or validation failure encountered while saving or loading a chunk
#[derive(Debug)]
pub enum ChunkError {
    /// The serialized chunk contains more entries than the chunk can hold
    CellCapacityExceeded,
    /// A serialized cell index is outside the chunk
    CellIndexOutOfRange,
    /// A serialized cell occurs more than once
    CellIndexIsDuplicate,
    /// Cells with an empty material identifier and no color must not be stored in sparse chunks
    CellIsEmpty,
    /// The serialized chunk coordinates do not match the requested coordinates
    CoordinateMismatch,
    /// The chunk file could not be read or written
    Io(io::Error),
    /// The serialized chunk header is invalid
    InvalidHeader,
    /// The serialized chunk data is truncated or has trailing bytes
    InvalidLength,
    /// The serialized chunk version is unsupported
    InvalidVersion,
}
