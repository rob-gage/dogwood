// Copyright Rob Gage 2026

use super::build;
use std::{
    error::Error,
    io, 
    path::PathBuf
};

/// Implementation of the `dogwood edit` command
pub fn edit(cargo_path: PathBuf, path: PathBuf, verbose: bool) -> Result<(), Box<dyn Error>> {
    let _build_directory: PathBuf = build(cargo_path, path.clone(), false, None, verbose);
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!(
            "Dogwood editor is not available yet; requested project: {}",
            path.display()
        ),
    ).into())
}
