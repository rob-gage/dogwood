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

    /// Creates the required project and engine dependency values
    pub fn new(project_name: String) -> Result<Self, Box<dyn Error>> {
        let engine_dependency: String =
            if let Some(version) = std::env::var_os("DOGWOOD_ENGINE_VERSION") {
                let version: String = version
                    .into_string()
                    .map_err(|_| "DOGWOOD_ENGINE_VERSION must be valid UTF-8")?;
                let parsed_version: Version = Version::parse(&version)?;
                format!("{{ version = \"{parsed_version}\" }}")
            } else {
                let engine_path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("engine")
                    .canonicalize()?;
                let manifest_path: String = engine_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .replace('"', "\\\"");
                format!("{{ path = \"{manifest_path}\" }}")
            };
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        values.insert("project_name".to_owned(), project_name);
        values.insert("engine_dependency".to_owned(), engine_dependency);
        Ok(Self { values })
    }

}
