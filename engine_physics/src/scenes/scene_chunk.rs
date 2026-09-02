// Copyright Rob Gage 2026

use crate::tiles::{
    TileInactive,
    TilePosition,
};

/// An inactive 64 tile by 64 tile chunk of a `Scene`
pub struct SceneChunk {
    /// The `TilePosition` of the bottom left tile in this `SceneChunk`
    tile_position: TilePosition,
    /// The tiles in this `SceneChunk`,
    tiles: [[TileInactive; 8]; 8],
}

impl SceneChunk {

    /// Returns the `TileInactive` at a given position in this `SceneChunk` if it is not out
    /// of bounds
    pub fn get_tile(&self, position: TilePosition) -> Result<&TileInactive, ()> {
        let x: usize = usize::try_from(
            position.x.checked_sub(self.tile_position.x).ok_or(())?
        ).map_err(|_| ())?;
        let y: usize = usize::try_from(
            position.y.checked_sub(self.tile_position.y).ok_or(())?
        ).map_err(|_| ())?;
        if x >= self.tiles[0].len() || y >= self.tiles.len() { return Err(()); }
        Ok(&self.tiles[y][x])
    }

    /// Returns the `TileInactive` at a given position in this `SceneChunk` if it is not out of
    /// bounds, panicking if it is
    pub fn get_tile_unchecked(&self, position: TilePosition) -> &TileInactive {
        self.get_tile(position).unwrap()
    }

    /// Sets a provided `TileInactive` at a given position in this `SceneChunk` if it is not out of
    /// bounds
    pub fn set_tile(&mut self, position: TilePosition, tile: TileInactive) -> Result<(), ()> {
        let x: usize = usize::try_from(
            position.x.checked_sub(self.tile_position.x).ok_or(())?
        ).map_err(|_| ())?;
        let y: usize = usize::try_from(
            position.y.checked_sub(self.tile_position.y).ok_or(())?
        ).map_err(|_| ())?;
        if x >= self.tiles[0].len() || y >= self.tiles.len() { return Err(()); }
        self.tiles[y][x] = tile;
        Ok(())
    }

    /// Sets a provided `TileInactive` at a given position in this `SceneChunk` if it is not out of
    /// bounds, panicking if it is
    pub fn set_tile_unchecked(&mut self, position: TilePosition, tile: TileInactive) {
        self.set_tile(position, tile).unwrap()
    }

}
