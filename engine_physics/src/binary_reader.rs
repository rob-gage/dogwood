// Copyright Rob Gage 2026

//! Shared little-endian primitive readers for persistence formats.

use std::io;

/// Reads one little-endian `u32` from a stream.
pub(crate) fn read_u32<R: io::Read>(reader: &mut R) -> Result<u32, io::Error> {
    let mut little_endian_u32_bytes: [u8; 4] = [0; 4];
    reader.read_exact(&mut little_endian_u32_bytes)?;
    Ok(u32::from_le_bytes(little_endian_u32_bytes))
}

/// Reads one little-endian `u32` from an in-memory byte slice.
pub(crate) fn read_u32_at(serialized_bytes: &[u8], byte_offset: usize) -> u32 {
    u32::from_le_bytes(
        serialized_bytes[byte_offset..byte_offset + 4]
            .try_into()
            .unwrap(),
    )
}
