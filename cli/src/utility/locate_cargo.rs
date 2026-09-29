// Copyright Rob Gage 2026

use semver::Version;
use std::{
    path::PathBuf,
    process::Command
};

/// Locates a valid installation of Cargo on the system that can be used with the engine
pub fn locate_cargo() -> Result<PathBuf, ()> {
    // find Cargo through PATH
    if let Ok(path) = which::which("cargo") {
        if let Ok(output) = Command::new(&path).arg("--version").output() {
            if output.status.success() {
                let output: String = String::from_utf8_lossy(&output.stdout).into_owned();
                let has_version: bool = output.split("cargo ").skip(1).any(|output| {
                    output
                        .split_whitespace()
                        .next()
                        .is_some_and(|version| Version::parse(version).is_ok())
                });
                if has_version {
                    return Ok(path);
                }
            }
        }
    }
    // fall back to Cargo home
    if let Ok(cargo_home) = home::cargo_home() {
        let executable: &str = if cfg!(windows) { "cargo.exe" } else { "cargo" };
        let path: PathBuf = cargo_home.join("bin").join(executable);
        if let Ok(output) = Command::new(&path).arg("--version").output() {
            if output.status.success() {
                let output: String = String::from_utf8_lossy(&output.stdout).into_owned();
                let has_version: bool = output.split("cargo ").skip(1).any(|output| {
                    output
                        .split_whitespace()
                        .next()
                        .is_some_and(|version| Version::parse(version).is_ok())
                });
                if has_version {
                    return Ok(path);
                }
            }
        }
    }
    eprintln!(
        "Could not find a working Cargo installation.\n\
        Install Rust with rustup: \x1b]8;;https://rustup.rs\x1b\\https://rustup.rs\x1b]8;;\x1b\\\n\
        or add Cargo to your PATH."
    );
    Err(())
}
