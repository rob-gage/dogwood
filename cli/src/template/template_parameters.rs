// Copyright Rob Gage 2026

use std::{
    collections::BTreeMap,
    error::Error,
    path::PathBuf,
};
use semver::Version;

/// Values substituted into canonical template text files
pub struct TemplateParameters {
    /// Named values substituted into template text files
    pub values: BTreeMap<String, String>,
}

impl TemplateParameters {

    /// Creates the required project, engine, and optional editor dependency values
    pub fn new(project_name: String) -> Result<Self, Box<dyn Error>> {
        let (engine_dependency, editor_dependency): (String, String) =
            if let Some(version) = std::env::var_os("DOGWOOD_ENGINE_VERSION") {
                let version: String = version
                    .into_string()
                    .map_err(|_| "DOGWOOD_ENGINE_VERSION must be valid UTF-8")?;
                let parsed_version: Version = Version::parse(&version)?;
                (
                    format!("{{ version = \"{parsed_version}\" }}"),
                    format!("{{ version = \"{parsed_version}\", optional = true }}"),
                )
            } else {
                let engine_path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("engine")
                    .canonicalize()?;
                let manifest_path: String = engine_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .replace('"', "\\\"");
                let editor_path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("editor")
                    .canonicalize()?;
                let editor_manifest_path: String = editor_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .replace('"', "\\\"");
                (
                    format!("{{ path = \"{manifest_path}\" }}"),
                    format!("{{ path = \"{editor_manifest_path}\", optional = true }}"),
                )
            };
        let crate_name: String = project_name.replace('-', "_");
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        values.insert("project_name".to_owned(), project_name);
        values.insert("crate_name".to_owned(), crate_name);
        values.insert("engine_dependency".to_owned(), engine_dependency);
        values.insert("editor_dependency".to_owned(), editor_dependency);
        Ok(Self { values })
    }

}
