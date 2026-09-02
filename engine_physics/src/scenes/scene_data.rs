// Copyright Rob Gage 2026

use crate::tiles::TileCoordinates;
use super::SceneChunk;
use std::{
    fs::create_dir_all,
    io,
    path::PathBuf,
};

/// A source of persistent `Scene` data on the filesystem
pub struct SceneData {
    /// The path of the directory containing the data
    path: PathBuf,
}

impl SceneData {

    /// Opens a directory as `SceneData`, failing if the directory is not accessible, creating it
    /// if it does not exist
    pub fn open(path: PathBuf) -> Result<Self, io::Error> {
        create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Loads a `SceneChunk` from the `SceneData`, returning `None` if the chunk does not exist
    pub fn load_chunk(&self, position: TileCoordinates) -> Result<Option<SceneChunk>, io::Error> {
        todo!()
    }

    /// Saves a `SceneChunk` to the `SceneData`
    pub fn save_chunk(&self, chunk: &SceneChunk) -> Result<(), io::Error> {
        todo!()
    }

}
