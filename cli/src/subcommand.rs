// Copyright Rob Gage 2026

use std::path::PathBuf;

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
}
