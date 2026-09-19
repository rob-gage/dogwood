// Copyright Rob Gage 2026

use crate::{project::DogwoodProject, target::BuildTarget};
use std::{
    error::Error,
    path::PathBuf,
    process::{Command, Stdio},
};

pub(crate) struct BuildArtifact {
    pub(crate) executable: PathBuf,
}

pub(crate) fn build(
    project: &DogwoodProject,
    target: BuildTarget,
) -> Result<BuildArtifact, Box<dyn Error>> {
    let destination = project.root.join("dist").join(target.name());
    std::fs::create_dir_all(&destination)?;
    let dockerfile = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../build/Dockerfile");
    let output = format!("type=local,dest={}", destination.display());
    let mut command = Command::new("docker");
    command
        .args(["buildx", "build", "--file"])
        .arg(&dockerfile)
        .arg("--build-arg")
        .arg(format!("DOGWOOD_TARGET={}", target.name()))
        .arg("--build-arg")
        .arg(format!("RUST_TARGET={}", target.triple()))
        .arg("--build-arg")
        .arg(format!("GAME_PACKAGE={}", project.game_package))
        .arg("--build-arg")
        .arg(format!("GAME_BINARY={}", project.game_binary))
        .arg("--output")
        .arg(&output)
        .arg("--tag")
        .arg(format!("dogwood-{}", target.name()))
        .args(["--progress", "plain"]);
    if let Some(feature) = target.engine_compute_feature() {
        command
            .arg("--build-arg")
            .arg(format!("DOGWOOD_ENGINE_FEATURE={feature}"));
    }
    let status = command
        .arg(&project.root)
        .stdin(Stdio::null())
        .status()
        .map_err(|error| {
            format!(
                "failed to start Docker build for {} ({target}): {error}",
                project.root.display()
            )
        })?;
    if !status.success() {
        return Err(format!(
            "Docker build failed for {} ({target}) with {status}",
            project.root.display()
        )
        .into());
    }
    Ok(BuildArtifact {
        executable: destination.join(target.executable_name()),
    })
}

pub(crate) fn command_available(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) fn infrastructure_present() -> bool {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../build/Dockerfile")
        .is_file()
}
