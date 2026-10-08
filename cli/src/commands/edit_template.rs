// Copyright Rob Gage 2026

use super::edit::edit;
use crate::template::{
    TemplateCache,
    TemplateParameters,
    TemplateSource,
};
use std::{
    error::Error,
    path::PathBuf,
};

/// Opens the cached canonical template through the shared edit command
pub fn edit_template(cargo_path: PathBuf, verbose: bool) -> Result<(), Box<dyn Error>> {
    let source: TemplateSource = TemplateSource::canonical();
    let parameters: TemplateParameters =
        TemplateParameters::new("dogwood_template".to_owned())?;
    let cache: TemplateCache = TemplateCache::new(TemplateCache::default_root()?);
    let project_path: PathBuf = cache.select(&source, &parameters)?;
    edit(cargo_path, project_path, verbose)
}
