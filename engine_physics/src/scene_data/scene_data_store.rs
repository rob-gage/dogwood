// Copyright Rob Gage 2026

use super::scene_data_dormant_rigid::{DormantRigidBody, owner_chunk};
use crate::{chunks::Chunk, materials::MaterialRegistry, tiles::TileCoordinates};
use std::{
    collections::HashSet,
    fs::{File, create_dir, create_dir_all, metadata},
    io::{self, Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
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
        let home: PathBuf = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "Home directory is unavailable")
        })?;
        let save_directory: PathBuf = home.join(".DOGWOOD");
        create_dir_all(&save_directory)?;
        loop {
            let identifier: u64 = TEMPORARY_IDENTIFIER.fetch_add(1, Ordering::Relaxed);
            let path: PathBuf =
                save_directory.join(format!("dogwood-scene-{}-{identifier}", std::process::id(),));
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
    pub fn materials(&self) -> &MaterialRegistry {
        &self.materials
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

    /// Reads the sole rigid-record file for an origin chunk.  This is separate
    /// from chunk payloads because a rigid may overlap chunks it does not own.
    pub(crate) fn read_dormant_rigids(
        &self,
        owner: TileCoordinates,
    ) -> Result<Vec<DormantRigidBody>, io::Error> {
        let path = self.rigid_path(owner);
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        let legacy = match &magic {
            b"dwrigid1" => true,
            b"dwrigid2" => false,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid dormant rigid file",
                ));
            }
        };
        let mut count = [0; 4];
        file.read_exact(&mut count)?;
        let count = usize::try_from(u32::from_le_bytes(count)).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "dormant rigid count overflow")
        })?;
        if count > 1_024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "too many dormant rigid bodies",
            ));
        }
        let mut records = Vec::new();
        let mut ids = HashSet::new();
        records.try_reserve_exact(count).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "dormant rigid allocation failed",
            )
        })?;
        for _ in 0..count {
            let record = if legacy {
                DormantRigidBody::deserialize_legacy(&mut file, self.materials())?
            } else {
                DormantRigidBody::deserialize(&mut file, self.materials())?
            };
            let record_owner = owner_chunk(
                record.position,
                record.rotation,
                record.cells.iter().map(|cell| cell.local),
            )
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid rigid geometry"))?;
            let legacy_owner = TileCoordinates {
                x: record.position[0].floor() as i32,
                y: record.position[1].floor() as i32,
            }
            .chunk_coordinates();
            if record_owner != owner && legacy_owner != owner {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "dormant rigid is in the wrong owner file",
                ));
            }
            if !ids.insert(record.identifier) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "duplicate dormant rigid identity",
                ));
            }
            records.push(record);
        }
        Ok(records)
    }

    /// Atomically replaces one canonical owner file.  An empty record set
    /// removes the file, so destroyed/reloaded bodies cannot resurrect.
    pub(crate) fn write_dormant_rigids(
        &self,
        owner: TileCoordinates,
        records: &[DormantRigidBody],
    ) -> Result<(), io::Error> {
        let path = self.rigid_path(owner);
        if records.is_empty() {
            return match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            };
        }
        create_dir_all(path.parent().unwrap())?;
        let temporary = path.with_extension("rigid.tmp");
        {
            let mut file = File::create(&temporary)?;
            file.write_all(b"dwrigid2")?;
            let count = u32::try_from(records.len()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "too many dormant rigid bodies")
            })?;
            file.write_all(&count.to_le_bytes())?;
            for record in records {
                record.serialize(&mut file, self.materials())?;
            }
            file.sync_all()?;
        }
        std::fs::rename(temporary, path)
    }

    /// Startup-only scan establishes a collision-free monotonic scene ID.
    pub(crate) fn next_dormant_rigid_id(&self) -> Result<u64, io::Error> {
        let directory = self.path.join("rigids");
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(1),
            Err(error) => return Err(error),
        };
        let mut next = 1u64;
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("rigid") {
                continue;
            }
            let mut file = File::open(path)?;
            let mut magic = [0; 8];
            file.read_exact(&mut magic)?;
            let legacy = match &magic {
                b"dwrigid1" => true,
                b"dwrigid2" => false,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid dormant rigid file",
                    ));
                }
            };
            let mut count = [0; 4];
            file.read_exact(&mut count)?;
            let count = u32::from_le_bytes(count);
            if count > 1_024 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "too many dormant rigid bodies",
                ));
            }
            for _ in 0..count {
                let record = if legacy {
                    DormantRigidBody::deserialize_legacy(&mut file, self.materials())?
                } else {
                    DormantRigidBody::deserialize(&mut file, self.materials())?
                };
                next = next.max(record.identifier.checked_add(1).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "rigid identity overflow")
                })?);
            }
        }
        Ok(next)
    }

    /// Returns the `PathBuf` for `Chunk`s in this `SceneData`
    fn chunk_path(&self, position: TileCoordinates) -> PathBuf {
        self.path.join("chunks").join(format!(
            "{:08x}{:08x}.chunk",
            position.x as u32, position.y as u32,
        ))
    }

    fn rigid_path(&self, position: TileCoordinates) -> PathBuf {
        self.path.join("rigids").join(format!(
            "{:08x}{:08x}.rigid",
            position.x as u32, position.y as u32
        ))
    }
}
