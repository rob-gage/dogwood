// Copyright Rob Gage 2026

use super::{
    template_parameters::TemplateParameters,
    template_source::TemplateSource,
};
use std::{
    error::Error,
    fs,
    io,
    path::{
        Component,
        Path,
        PathBuf,
    },
};

/// Stable directory containing the generated template crate
const TEMPLATE_CRATE: &str = "crate";

/// File recording the inputs used for the cached project
const TEMPLATE_FINGERPRINT: &str = ".dogwood-template-fingerprint";

/// Generation format identifier included in cache fingerprints
const TEMPLATE_FORMAT_STRING: &str = "dogwood-template-format-1";

/// Directory holding the last generated contents used to detect user edits
const TEMPLATE_STATE: &str = ".dogwood-template-state";

/// Temporary directory used while updating generated content snapshots
const TEMPLATE_STATE_STAGING: &str = ".dogwood-template-state-staging";

/// Selects one stable generated template project from a temporary directory
pub struct TemplateCache {
    /// Root containing the stable project and its generation state
    root: PathBuf,
}

impl TemplateCache {

    /// Creates a cache at a specific root, useful for isolated callers
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Returns the stable operating system temporary root for the template project
    pub fn default_root() -> Result<PathBuf, Box<dyn Error>> {
        Ok(std::env::temp_dir().join("dogwood").join("template"))
    }

    /// Reuses one cached project and safely updates files when its template changes
    pub fn select(
        &self,
        source: &TemplateSource,
        parameters: &TemplateParameters,
    ) -> Result<PathBuf, Box<dyn Error>> {
        let template_crate_path: PathBuf = self.root.join(TEMPLATE_CRATE);
        let template_state_path: PathBuf = self.root.join(TEMPLATE_STATE);
        let template_state_staging_path: PathBuf = self.root.join(TEMPLATE_STATE_STAGING);
        let template_fingerprint_path: PathBuf = self.root.join(TEMPLATE_FINGERPRINT);
        let fingerprint: String = Self::fingerprint(source, parameters)?;
        let project_exists: bool = match fs::symlink_metadata(&template_crate_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "cached template path {} is a symlink; it was preserved",
                        template_crate_path.display()
                    ),
                ).into());
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "cached template path {} is not a directory; it was preserved",
                        template_crate_path.display()
                    ),
                ).into());
            }
            Ok(_) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        if project_exists {
            let marker: String = fs::read_to_string(&template_fingerprint_path).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "cached template at {} has no generation marker; it was preserved",
                        template_crate_path.display()
                    ),
                )
            })?;
            if marker == fingerprint {
                return Ok(template_crate_path);
            }
        }
        let rendered_files: Vec<(PathBuf, Vec<u8>)> = source.rendered_files(parameters)?;
        fs::create_dir_all(&self.root)?;
        if template_state_staging_path.exists() {
            fs::remove_dir_all(&template_state_staging_path)?;
        }
        fs::create_dir_all(&template_state_staging_path)?;
        let created_project: bool = !project_exists;
        if created_project {
            fs::create_dir_all(&template_crate_path)?;
        }
        let update_result: Result<(), Box<dyn Error>> = (|| {
            for file_entry in &rendered_files {
                let relative_path: &PathBuf = &file_entry.0;
                let contents: &Vec<u8> = &file_entry.1;
                let project_file: PathBuf = template_crate_path.join(relative_path);
                let previous_file: PathBuf = template_state_path.join(relative_path);
                let next_state_file: PathBuf = template_state_staging_path.join(relative_path);
                let safe_project_path: bool = !Self::contains_symlink(
                    &template_crate_path,
                    relative_path,
                )?;
                if safe_project_path && project_file.is_file() {
                    let can_update: bool = previous_file.is_file()
                        && fs::read(&project_file)? == fs::read(&previous_file)?;
                    if can_update {
                        Self::write_file(&project_file, contents)?;
                    }
                } else if safe_project_path && !project_file.exists() {
                    Self::write_file(&project_file, contents)?;
                }
                Self::write_file(&next_state_file, contents)?;
            }
            Ok(())
        })();
        if let Err(error) = update_result {
            let _ = fs::remove_dir_all(&template_state_staging_path);
            if created_project {
                let _ = fs::remove_dir_all(&template_crate_path);
            }
            return Err(error);
        }
        if template_state_path.exists() {
            fs::remove_dir_all(&template_state_path)?;
        }
        fs::rename(&template_state_staging_path, &template_state_path)?;
        fs::write(&template_fingerprint_path, fingerprint.as_bytes())?;
        Ok(template_crate_path)
    }

    /// Returns the deterministic key for a template and its generation inputs
    pub fn fingerprint(
        source: &TemplateSource,
        parameters: &TemplateParameters,
    ) -> Result<String, Box<dyn Error>> {
        let mut first_hash: u64 = 0xcbf29ce484222325;
        let mut second_hash: u64 = 0x84222325cbf29ce4;
        Self::hash_bytes(
            &mut first_hash,
            &mut second_hash,
            TEMPLATE_FORMAT_STRING.as_bytes(),
        );
        for file_entry in source.files()? {
            let relative_path: String = file_entry.0.to_string_lossy().into_owned();
            let contents: Vec<u8> = file_entry.1;
            Self::hash_bytes(&mut first_hash, &mut second_hash, relative_path.as_bytes());
            Self::hash_bytes(&mut first_hash, &mut second_hash, &contents);
        }
        for (field_name, field_value) in &parameters.values {
            Self::hash_bytes(&mut first_hash, &mut second_hash, field_name.as_bytes());
            Self::hash_bytes(&mut first_hash, &mut second_hash, field_value.as_bytes());
        }
        Ok(format!("{first_hash:016x}{second_hash:016x}"))
    }

    /// Adds length-delimited bytes to both fingerprint accumulators
    fn hash_bytes(first_hash: &mut u64, second_hash: &mut u64, bytes: &[u8]) {
        for byte in (bytes.len() as u64)
            .to_le_bytes()
            .iter()
            .chain(bytes.iter())
            .chain([0xff].iter())
        {
            *first_hash ^= u64::from(*byte);
            *first_hash = first_hash.wrapping_mul(0x100000001b3);
            *second_hash ^= u64::from(*byte).wrapping_add(*first_hash >> 32);
            *second_hash = second_hash.wrapping_mul(0x100000001b3);
        }
    }

    /// Checks whether a cached project path crosses a symbolic link
    fn contains_symlink(root: &Path, relative_path: &Path) -> Result<bool, Box<dyn Error>> {
        let mut current_path: PathBuf = root.to_path_buf();
        for component in relative_path.components() {
            if !matches!(component, Component::Normal(_)) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unsafe cached template path: {}", relative_path.display()),
                ).into());
            }
            current_path.push(component.as_os_str());
            match fs::symlink_metadata(&current_path) {
                Ok(metadata) if metadata.file_type().is_symlink() => return Ok(true),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(false)
    }

    /// Creates parent directories and writes one cache-managed file
    fn write_file(path: &Path, contents: &[u8]) -> Result<(), Box<dyn Error>> {
        let parent: &Path = path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "template file has no parent directory")
        })?;
        fs::create_dir_all(parent)?;
        fs::write(path, contents)?;
        Ok(())
    }

}
