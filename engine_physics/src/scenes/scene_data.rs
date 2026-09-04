// Copyright Rob Gage 2026

use crate::{
    chunks::Chunk,
    tiles::TileCoordinates,
};
use std::{
    fs::{
        create_dir_all,
        File,
    },
    io,
    path::PathBuf,
};

/// A source of persistent `Scene` data on the filesystem
#[derive(Clone)]
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

    /// Reads a `Chunk` from the `SceneData`, returning `None` if the chunk does not exist
    pub fn read_chunk(&self, position: TileCoordinates) -> Result<Option<Chunk>, io::Error> {
        let path: PathBuf = self.chunk_path(position);
        let mut file: File = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        Ok(Some(Chunk::deserialize(&mut file)?))
    }

    /// Writes a `Chunk` to the `SceneData`
    pub fn write_chunk(&self, chunk: &Chunk) -> Result<(), io::Error> {
        let position: TileCoordinates = chunk.tile_coordinates;
        let path: PathBuf = self.chunk_path(position);
        create_dir_all(path.parent().unwrap())?;
        let mut file: File = File::create(path)?;
        chunk.serialize(&mut file)
    }

    /// Returns the `PathBuf` for `Chunk`s in this `SceneData`
    fn chunk_path(&self, position: TileCoordinates) -> PathBuf {
        self.path.join("chunks").join(format!(
            "{:08x}{:08x}.chunk",
            position.x as u32,
            position.y as u32,
        ))
    }

}
