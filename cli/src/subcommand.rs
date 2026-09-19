// Copyright Rob Gage 2026

use std::path::PathBuf;

use crate::commands::TargetFlags;

#[derive(clap::Subcommand)]
pub(super) enum Subcommand {
    /// Create a runnable Dogwood project.
    New {
        /// Directory to create the project in.
        #[arg(default_value = ".", value_name = "DIRECTORY")]
        directory: PathBuf,
        /// Project/package name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Run the game with Cargo's debug profile.
    Debug {
        #[arg(default_value = ".", value_name = "DIRECTORY")]
        directory: PathBuf,
    },
    /// Build an optimized distributable artifact; target flags are one platform
    /// plus one architecture, or omitted to use the current target.
    Build {
        #[arg(default_value = ".", value_name = "DIRECTORY")]
        directory: PathBuf,
        #[command(flatten)]
        target: TargetFlags,
    },
    /// Build and execute an optimized artifact using the same target flags as
    /// `build`.
    Run {
        #[arg(default_value = ".", value_name = "DIRECTORY")]
        directory: PathBuf,
        #[command(flatten)]
        target: TargetFlags,
    },
    /// Validate a Dogwood project and its build tools.
    Check {
        #[arg(default_value = ".", value_name = "DIRECTORY")]
        directory: PathBuf,
    },
}
