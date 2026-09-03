// Copyright Rob Gage 2026

use crate::tiles::{
    TileCoordinates,
    TileData,
};
use std::{
    array,
    io
};

/// An inactive 64 tile by 64 tile chunk of a `Scene`
pub struct SceneChunk {
    /// The `TilePosition` of the bottom left tile in this `SceneChunk`
    pub tile_coordinates: TileCoordinates,
    /// The tiles in this `SceneChunk`,
    tiles: [[TileData; 8]; 8],
}

impl SceneChunk {

    /// Creates a new empty `SceneChunk`
    pub fn new_empty(tile_coordinates: TileCoordinates) -> Self {
        Self {
            tile_coordinates,
            tiles: array::from_fn(|_| array::from_fn(|_| TileData::EMPTY))
        }
    }

    /// Deserializes binary data into a `SceneChunk`
    pub fn deserialize<R: io::Read>(reader: &mut R) -> Result<SceneChunk, io::Error> {
        let mut magic: [u8; 8] = [0; 8];
        reader.read_exact(&mut magic)?;
        if &magic != b"dogwood_" { return Err(io::ErrorKind::InvalidData.into()); }
        let mut coordinate_data: [u8; 8] = [0; 8];
        reader.read_exact(&mut coordinate_data)?;
        let tile_coordinates: TileCoordinates = TileCoordinates {
            x: i32::from_le_bytes(coordinate_data[0..4].try_into().unwrap()),
            y: i32::from_le_bytes(coordinate_data[4..8].try_into().unwrap()),
        };
        let mut tile_data: Vec<TileData> = Vec::with_capacity(64);
        for _ in 0..64 { tile_data.push(TileData::deserialize(reader)?); }
        let mut tile_data = tile_data.into_iter();
        let tiles: [[TileData; 8]; 8] = array::from_fn(|_| {
            array::from_fn(|_| tile_data.next().unwrap())
        });
        Ok(Self { tile_coordinates, tiles })
    }

    /// Serializes a `SceneChunk` into binary data
    pub fn serialize<W: io::Write>(&self, writer: &mut W) -> Result<(), io::Error> {
        writer.write_all(b"dogwood_")?;
        writer.write_all(&self.tile_coordinates.x.to_le_bytes())?;
        writer.write_all(&self.tile_coordinates.y.to_le_bytes())?;
        for row in &self.tiles {
            for tile in row {
                tile.serialize(writer)?;
            }
        }
        Ok(())
    }

    /// Returns the `TileInactive` at a given position in this `SceneChunk` if it is not out
    /// of bounds
    pub fn get_tile(&self, position: TileCoordinates) -> Result<&TileData, ()> {
        let x: usize = usize::try_from(
            position.x.checked_sub(self.tile_coordinates.x).ok_or(())?
        ).map_err(|_| ())?;
        let y: usize = usize::try_from(
            position.y.checked_sub(self.tile_coordinates.y).ok_or(())?
        ).map_err(|_| ())?;
        if x >= self.tiles[0].len() || y >= self.tiles.len() { return Err(()); }
        Ok(&self.tiles[y][x])
    }

    /// Returns the `TileInactive` at a given position in this `SceneChunk` if it is not out of
    /// bounds, panicking if it is
    pub fn get_tile_unchecked(&self, position: TileCoordinates) -> &TileData {
        self.get_tile(position).unwrap()
    }

    /// Sets a provided `TileInactive` at a given position in this `SceneChunk` if it is not out of
    /// bounds
    pub fn set_tile(&mut self, position: TileCoordinates, tile: TileData) -> Result<(), ()> {
        let x: usize = usize::try_from(
            position.x.checked_sub(self.tile_coordinates.x).ok_or(())?
        ).map_err(|_| ())?;
        let y: usize = usize::try_from(
            position.y.checked_sub(self.tile_coordinates.y).ok_or(())?
        ).map_err(|_| ())?;
        if x >= self.tiles[0].len() || y >= self.tiles.len() { return Err(()); }
        self.tiles[y][x] = tile;
        Ok(())
    }

    /// Sets a provided `TileInactive` at a given position in this `SceneChunk` if it is not out of
    /// bounds, panicking if it is
    pub fn set_tile_unchecked(&mut self, position: TileCoordinates, tile: TileData) {
        self.set_tile(position, tile).unwrap()
    }

}
