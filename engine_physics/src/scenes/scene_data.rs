// Copyright Rob Gage 2026

use crate::{
    chunks::Chunk,
    materials::MaterialRegistry,
    tiles::TileCoordinates,
};
use std::{
    fs::{
        create_dir,
        create_dir_all,
        metadata,
        remove_dir_all,
        File,
    },
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{
            AtomicU64,
            Ordering,
        },
    },
};

/// The identifier assigned to the next temporary scene-data directory
static TEMPORARY_IDENTIFIER: AtomicU64 = AtomicU64::new(0);

/// A source of filesystem-backed `Scene` data
#[derive(Clone)]
pub struct SceneData {
    /// The CPU-side materials registered for this scene
    materials: Arc<MaterialRegistry>,
    /// The path of the directory containing the data
    path: Arc<PathBuf>,
    /// Whether the directory should be removed after its last owner is dropped
    is_temporary: bool,
}

impl SceneData {

    /// Loads scene data from an existing directory
    pub fn load(path: PathBuf) -> Result<Self, io::Error> {
        if !metadata(&path)?.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Scene data path is not a directory",
            ));
        }
        let mut materials_file: File = File::open(path.join("materials"))?;
        let materials: MaterialRegistry = MaterialRegistry::deserialize(&mut materials_file)?;
        Ok(Self {
            materials: Arc::new(materials),
            path: Arc::new(path),
            is_temporary: false,
        })
    }

    /// Creates empty scene data in a new temporary directory
    pub fn new_temporary(materials: MaterialRegistry) -> Result<Self, io::Error> {
        let materials: Arc<MaterialRegistry> = Arc::new(materials);
        loop {
            let identifier: u64 = TEMPORARY_IDENTIFIER.fetch_add(1, Ordering::Relaxed);
            let path: PathBuf = std::env::temp_dir().join(format!(
                "dogwood-scene-{}-{identifier}",
                std::process::id(),
            ));
            match create_dir(&path) {
                Ok(()) => {
                    let mut materials_file: File = File::create(path.join("materials"))?;
                    materials.serialize(&mut materials_file)?;
                    return Ok(Self {
                        materials,
                        path: Arc::new(path),
                        is_temporary: true,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
    }

    /// Returns the CPU-side materials stored with this scene data
    pub fn materials(&self) -> &MaterialRegistry { &self.materials }

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