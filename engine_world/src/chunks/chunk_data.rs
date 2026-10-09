// Copyright Rob Gage 2026

use super::{
    Chunk,
    ChunkCoordinates,
    ChunkError,
};
use std::{
    fs,
    io,
    path::PathBuf,
    sync::{
        atomic::{
            AtomicU64,
            Ordering,
        },
        Arc,
    },
};

/// Reads and writes versioned chunk files in one scene directory
#[derive(Clone, Debug)]
pub struct ChunkData {
    /// Directory holding the persisted chunk files
    path: PathBuf,
    /// Sequence shared by clones of this chunk data store
    next_temporary_file_sequence: Arc<AtomicU64>,
}

impl ChunkData {

    /// Creates chunk data storage rooted at the given directory
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            path: root.into(),
            next_temporary_file_sequence: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns the directory containing chunk files
    pub fn path(&self) -> &PathBuf { &self.path }

    /// Loads a chunk file or returns `None` when it does not exist
    pub fn load(&self, coordinates: ChunkCoordinates) -> Result<Option<Chunk>, ChunkError> {
        let path: PathBuf = self.chunk_file_path(coordinates);
        // missing files are left for the scene's generation path
        let mut file: fs::File = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ChunkError::Io(error)),
        };
        let chunk: Chunk = Chunk::read(&mut file)?;
        // ensure the coordinates match the filename
        if chunk.coordinates != coordinates { return Err(ChunkError::CoordinateMismatch); }
        Ok(Some(chunk))
    }

    /// Atomically writes a chunk file by replacing it with a synced temporary file
    pub fn save(&self, chunk: &Chunk) -> Result<(), ChunkError> {
        fs::create_dir_all(&self.path).map_err(ChunkError::Io)?;
        let destination: PathBuf = self.chunk_file_path(chunk.coordinates);
        let sequence: u64 = self.next_temporary_file_sequence
            .fetch_add(1, Ordering::Relaxed);
        // write to a unique sibling file so incomplete data cannot replace the saved chunk
        let temporary: PathBuf = self.path.join(format!(
            "{:08x}{:08x}_{}.chunk.temporary",
            chunk.coordinates.x as u32,
            chunk.coordinates.y as u32,
            sequence,
        ));
        let write_result: Result<(), ChunkError> = (|| {
            let mut file: fs::File = fs::File::options()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(ChunkError::Io)?;
            chunk.write(&mut file)?;
            file.sync_all().map_err(ChunkError::Io)?;
            // overwrite the existing file only after its contents are synced
            fs::rename(&temporary, &destination).map_err(ChunkError::Io)?;
            Ok(())
        })();
        if write_result.is_err() { let _ = fs::remove_file(&temporary); }
        write_result
    }

    /// Builds the stable file path for a chunk with provided `ChunkCoordinates`
    fn chunk_file_path(&self, coordinates: ChunkCoordinates) -> PathBuf {
        self.path.join(format!("{:08x}{:08x}.chunk", coordinates.x as u32, coordinates.y as u32))
    }

}
