// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;
use super::SceneChunk;
use std::{
    fs::{
        create_dir_all,
        File,
    },
    io,
    path::PathBuf,
};

/// A source of persistent `Scene` data on the filesystem
pub struct SceneData {
    /// The path of the directory containing the data
    path: PathBuf,
}

impl SceneData {

    fn chunk_path(&self, position: TileCoordinates) -> PathBuf {
        self.path.join("chunks").join(format!(
            "{:08}_{:08}.chunk",
            position.region_coordinates_x(),
            position.region_coordinates_y(),
        ))
    }

    /// Opens a directory as `SceneData`, failing if the directory is not accessible, creating it
    /// if it does not exist
    pub fn open(path: PathBuf) -> Result<Self, io::Error> {
        create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Loads a `SceneChunk` from the `SceneData`, returning `None` if the chunk does not exist
    pub fn load_chunk(&self, position: TileCoordinates) -> Result<Option<SceneChunk>, io::Error> {
        let path: PathBuf = self.chunk_path(position);
        let mut file: File = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(Some(SceneChunk::deserialize(&mut file)?))
    }

    /// Saves a `SceneChunk` to the `SceneData`
    pub fn save_chunk(&self, chunk: &SceneChunk) -> Result<(), io::Error> {
        let position: TileCoordinates = chunk.tile_coordinates;
        let path: PathBuf = self.chunk_path(position);
        create_dir_all(path.parent().unwrap())?;
        let mut file: File = File::create(path)?;
        chunk.serialize(&mut file)
    }

}
