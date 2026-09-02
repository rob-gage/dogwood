// Copyright Rob Gage 2026

use std::io;

/// An inactive 8x8 tile
pub struct TileInactive([[(); 8]; 8]);

impl TileInactive {

    /// Deserializes binary data into a `TileInactive`
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<TileInactive, io::Error> {
        let mut data: [u8; 64] = [0; 64];
        reader.read_exact(&mut data)?;
        Ok(Self([[(); 8]; 8]))
    }

    /// Serializes a `TileInactive` into binary data
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(&[0; 64])
    }

}
