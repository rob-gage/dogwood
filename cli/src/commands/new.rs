// Copyright Rob Gage 2026

use crate::template::{
    TemplateParameters,
    TemplateSource
};
use std::{
    error::Error,
    io,
    path::PathBuf
};

/// Creates an independent downstream project from the canonical template
pub fn new(name: String, destination: PathBuf) -> Result<(), Box<dyn Error>> {
    let mut characters: std::str::Chars<'_> = name.chars();
    let first_character: char = characters.next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Project name cannot be empty.")
    })?;
    if !first_character.is_ascii_alphanumeric() || !characters.all(|character| {
        character.is_ascii_alphanumeric() || character == '-' || character == '_'
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid project name '{name}';\
            use ASCII letters, digits, '-' or '_' and begin with a letter or digit"),
        ).into());
    }
    let uppercase_name: String = name.to_ascii_uppercase();
    let windows_device_name: bool =
        matches!(uppercase_name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || ["COM", "LPT"]
                .iter()
                .any(|prefix| (1..=9).any(|number| uppercase_name == format!("{prefix}{number}")));
    if windows_device_name {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{name}' is reserved as a project name on Windows"),
        ).into());
    }
    let source: TemplateSource = TemplateSource::canonical();
    let parameters: TemplateParameters = TemplateParameters::new(name)?;
    source.generate(&destination, &parameters)?;
    println!("Created Dogwood project at {}", destination.display());
    Ok(())
}
