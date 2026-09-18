// Copyright Rob Gage 2026

use cargo_metadata::{MetadataCommand, TargetKind};
use std::{
    error::Error,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(crate) struct DogwoodProject {
    pub(crate) root: PathBuf,
    pub(crate) game_package: String,
    pub(crate) game_binary: String,
}

impl DogwoodProject {
    pub(crate) fn discover(directory: &Path) -> Result<Self, Box<dyn Error>> {
        let directory = directory.canonicalize().map_err(|error| {
            format!(
                "cannot access project directory {}: {error}",
                directory.display()
            )
        })?;
        let manifest = directory.join("Cargo.toml");
        if !manifest.is_file() {
            return Err(format!(
                "{} does not contain a workspace Cargo.toml",
                directory.display()
            )
            .into());
        }

        let metadata = MetadataCommand::new()
            .manifest_path(&manifest)
            .no_deps()
            .exec()
            .map_err(|error| {
                format!(
                    "failed to read Cargo workspace {}: {error}",
                    manifest.display()
                )
            })?;
        let root = metadata.workspace_root.clone().into_std_path_buf();
        if root != directory {
            return Err(format!(
                "{} is not the workspace root; use {}",
                directory.display(),
                root.display()
            )
            .into());
        }

        let package_name = metadata
            .workspace_metadata
            .get("dogwood")
            .and_then(|value| value.get("game"))
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .or_else(|| {
                metadata.workspace_packages().iter()
                    .filter(|package| package.name == "game")
                    .count().eq(&1)
                    .then(|| "game".to_owned())
            })
            .ok_or_else(|| "workspace must define [workspace.metadata.dogwood].game or contain exactly one package named `game`".to_owned())?;
        let workspace_packages = metadata.workspace_packages();
        let package = workspace_packages
            .iter()
            .find(|package| package.name == package_name)
            .ok_or_else(|| {
                format!("Dogwood game package `{package_name}` is not in the workspace")
            })?;
        let binary = package
            .targets
            .iter()
            .find(|target| target.kind.iter().any(|kind| kind == &TargetKind::Bin))
            .ok_or_else(|| {
                format!(
                    "Dogwood package `{package_name}` does not contain an executable binary target"
                )
            })?;

        Ok(Self {
            root,
            game_package: package.name.clone(),
            game_binary: binary.name.clone(),
        })
    }
}
