// Copyright Rob Gage 2026

use crate::{
    materials::MaterialIdentifier,
    tiles::{
        CellularAppearance,
        TileCoordinates,
        TileData,
    },
};
use std::io;

/// An inactive 64 tile by 64 tile chunk of a `Scene`
pub struct Chunk {
    /// The `TilePosition` of the bottom left tile in this `Chunk`
    pub tile_coordinates: TileCoordinates,
    /// The tiles in this `Chunk`,
    tiles: Box<[TileData]>,
}

impl Chunk {

    /// The width and height of a `Chunk` in tiles
    pub const WIDTH: u16 = 64;

    /// Creates a new empty `Chunk`
    pub fn new_empty(tile_coordinates: TileCoordinates) -> Self {
        Self {
            tile_coordinates,
            tiles: (0..usize::from(Self::WIDTH) * usize::from(Self::WIDTH))
                .map(|_| TileData::EMPTY).collect(),
        }
    }

    /// Deserializes binary data into a `Chunk`
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<Chunk, io::Error> {
        let mut magic: [u8; 8] = [0; 8];
        reader.read_exact(&mut magic)?;
        if &magic != b"dogwood_" { return Err(io::ErrorKind::InvalidData.into()); }
        let mut coordinate_data: [u8; 8] = [0; 8];
        reader.read_exact(&mut coordinate_data)?;
        let tile_coordinates: TileCoordinates = TileCoordinates {
            x: i32::from_le_bytes(coordinate_data[0..4].try_into().unwrap()),
            y: i32::from_le_bytes(coordinate_data[4..8].try_into().unwrap()),
        };
        let mut tile_data: Vec<TileData> = Vec::with_capacity(4096);
        for _ in 0..4096 { tile_data.push(TileData::deserialize(reader)?); }
        Ok(Self { tile_coordinates, tiles: tile_data.into_boxed_slice() })
    }

    /// Serializes a `Chunk` into binary data
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(b"dogwood_")?;
        writer.write_all(&self.tile_coordinates.x.to_le_bytes())?;
        writer.write_all(&self.tile_coordinates.y.to_le_bytes())?;
        for tile in &self.tiles { tile.serialize(writer)?; }
        Ok(())
    }

    /// Returns the `TileData` at a given position in this `Chunk` if it is not out of bounds
    pub fn get_tile(&self, position: TileCoordinates) -> Result<&TileData, ()> {
        let x: usize = usize::try_from(
            position.x.checked_sub(self.tile_coordinates.x).ok_or(())?
        ).map_err(|_| ())?;
        let y: usize = usize::try_from(
            position.y.checked_sub(self.tile_coordinates.y).ok_or(())?
        ).map_err(|_| ())?;
        if x >= usize::from(Self::WIDTH) || y >= usize::from(Self::WIDTH) { return Err(()); }
        Ok(&self.tiles[y * usize::from(Self::WIDTH) + x])
    }

    /// Returns the `TileData` at a given position in this `Chunk`, panicking if it is out of bounds
    pub fn get_tile_unchecked(&self, position: TileCoordinates) -> &TileData {
        self.get_tile(position).unwrap()
    }

    /// Sets a provided `TileData` at a given position in this `Chunk` if it is not out of bounds
    pub fn set_tile(&mut self, position: TileCoordinates, tile: TileData) -> Result<(), ()> {
        let x: usize = usize::try_from(
            position.x.checked_sub(self.tile_coordinates.x).ok_or(())?
        ).map_err(|_| ())?;
        let y: usize = usize::try_from(
            position.y.checked_sub(self.tile_coordinates.y).ok_or(())?
        ).map_err(|_| ())?;
        if x >= usize::from(Self::WIDTH) || y >= usize::from(Self::WIDTH) { return Err(()); }
        self.tiles[y * usize::from(Self::WIDTH) + x] = tile;
        Ok(())
    }

    /// Sets a provided `TileData` at a given position in this `Chunk`,
    /// panicking if it is out of bounds
    pub fn set_tile_unchecked(&mut self, position: TileCoordinates, tile: TileData) {
        self.set_tile(position, tile).unwrap()
    }

    /// Sets one cell in a tile in this `Chunk` if the tile is not out of bounds
    pub fn set_cell(
        &mut self,
        position: TileCoordinates,
        x: usize,
        y: usize,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
    ) -> Result<(), ()> {
        self.set_cell_with_integrity(position, x, y, material_identifier, appearance, 0.0)
    }

    /// Sets one cell with an explicit persistent integrity value
    pub fn set_cell_with_integrity(
        &mut self,
        position: TileCoordinates,
        x: usize,
        y: usize,
        material_identifier: MaterialIdentifier,
        appearance: CellularAppearance,
        integrity: f32,
    ) -> Result<(), ()> {
        let tile_x: usize = usize::try_from(
            position.x.checked_sub(self.tile_coordinates.x).ok_or(())?
        ).map_err(|_| ())?;
        let tile_y: usize = usize::try_from(
            position.y.checked_sub(self.tile_coordinates.y).ok_or(())?
        ).map_err(|_| ())?;
        if tile_x >= usize::from(Self::WIDTH) || tile_y >= usize::from(Self::WIDTH) {
            return Err(());
        }
        self.tiles[tile_y * usize::from(Self::WIDTH) + tile_x].set_cell_with_integrity(
            x,
            y,
            material_identifier,
            appearance,
            integrity,
        );
        Ok(())
    }

}
