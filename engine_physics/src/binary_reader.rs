// Copyright Rob Gage 2026

use std::io;

pub(crate) fn read_u32<R: io::Read>(reader: &mut R) -> Result<u32, io::Error> {
    let mut bytes: [u8; 4] = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

pub(crate) fn read_u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
