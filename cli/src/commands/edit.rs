// Copyright Rob Gage 2026

use super::build;
use std::{
    error::Error,
    io::{
        self,
        Write,
    },
    path::PathBuf,
    process::{
        Command,
        exit,
        ExitStatus,
    },
};

/// Implementation of the `dogwood edit` command
pub fn edit(cargo_path: PathBuf, path: PathBuf, verbose: bool) -> Result<(), Box<dyn Error>> {
    let build_directory: PathBuf = build(
        cargo_path.clone(),
        path.clone(),
        false,
        None,
        verbose,
    );
    let mut command: Command = Command::new(cargo_path);
    command
        .arg("run")
        .arg("--release")
        .arg("--bin")
        .arg("editor")
        .arg("--features")
        .arg("editor")
        .arg("--target-dir")
        .arg(build_directory)
        .current_dir(path);
    if !verbose { command.arg("--quiet"); }
    if verbose {
        println!("Running command: {command:?}");
        let _ = io::stdout().flush();
    }
    let status: ExitStatus = command.status()?;
    if !status.success() { exit(status.code().unwrap_or(1)); }
    Ok(())
}
