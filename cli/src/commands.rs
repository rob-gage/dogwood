// Copyright Rob Gage 2026

use crate::{
    build::{
        build, build_to, command_available, fingerprint, run_cache_directory, validate_artifact,
    },
    project::DogwoodProject,
    target::BuildTarget,
};
use std::{error::Error, path::PathBuf, process::Command};

#[derive(clap::Args, Default)]
pub(crate) struct TargetFlags {
    #[arg(long, group = "target")]
    pub(crate) windows_x64: bool,
    #[arg(long, group = "target")]
    pub(crate) windows_x86: bool,
    #[arg(long, group = "target")]
    pub(crate) windows_arm64: bool,
    #[arg(long, group = "target")]
    pub(crate) linux_x64: bool,
    #[arg(long, group = "target")]
    pub(crate) linux_x86: bool,
    #[arg(long, group = "target")]
    pub(crate) linux_arm64: bool,
}

impl TargetFlags {
    pub(crate) fn target(&self) -> Result<BuildTarget, Box<dyn Error>> {
        let selected = [
            (self.windows_x64, "windows-x64"),
            (self.windows_x86, "windows-x86"),
            (self.windows_arm64, "windows-arm64"),
            (self.linux_x64, "linux-x64"),
            (self.linux_x86, "linux-x86"),
            (self.linux_arm64, "linux-arm64"),
        ];
        selected.iter().find(|(selected, _)| *selected).map_or_else(
            || BuildTarget::host().map_err(Into::into),
            |(_, name)| name.parse().map_err(Into::into),
        )
    }
}

pub(crate) fn debug(directory: PathBuf) -> Result<(), Box<dyn Error>> {
    let project = DogwoodProject::discover(&directory)?;
    let status = Command::new("cargo")
        .args(["run", "--package", &project.game_package])
        .current_dir(&project.root)
        .status()?;
    if !status.success() {
        return Err(format!(
            "cargo run failed for {} with {status}",
            project.root.display()
        )
        .into());
    }
    Ok(())
}

pub(crate) fn build_command(directory: PathBuf, flags: TargetFlags) -> Result<(), Box<dyn Error>> {
    let project = DogwoodProject::discover(&directory)?;
    let target = flags.target()?;
    let artifact = build(&project, target)?;
    println!("Built {target} package: {}", artifact.package_dir.display());
    Ok(())
}

pub(crate) fn run(directory: PathBuf, flags: TargetFlags) -> Result<(), Box<dyn Error>> {
    let project = DogwoodProject::discover(&directory)?;
    let target = flags.target()?;
    let package_dir = run_cache_directory(&project, target)?;
    let fingerprint_path = package_dir.join("dogwood-fingerprint");
    let current_fingerprint = fingerprint(&project, target)?;
    let cached = std::fs::read_to_string(&fingerprint_path)
        .is_ok_and(|value| value == current_fingerprint)
        && validate_artifact(
            &crate::build::BuildArtifact {
                package_dir: package_dir.clone(),
                executable: package_dir.join(target.executable_name()),
            },
            target,
        )
        .is_ok();
    let artifact = if cached {
        println!("Dogwood run: using cached {target} build");
        crate::build::BuildArtifact {
            package_dir: package_dir.clone(),
            executable: package_dir.join(target.executable_name()),
        }
    } else {
        println!("Dogwood run: project changed; rebuilding {target}");
        if package_dir.exists() {
            std::fs::remove_dir_all(&package_dir)?;
        }
        let artifact = build_to(&project, target, package_dir.clone())?;
        std::fs::write(&fingerprint_path, current_fingerprint)?;
        artifact
    };
    if target != BuildTarget::host()? {
        println!("Built {target} package: {}", artifact.package_dir.display());
        return Err(format!(
            "cannot execute {target} natively on this host; artifact is at {}",
            artifact.executable.display()
        )
        .into());
    }
    let status = Command::new(&artifact.executable)
        .current_dir(&artifact.package_dir)
        .status()?;
    if !status.success() {
        return Err(format!("game failed for {} with {status}", project.root.display()).into());
    }
    Ok(())
}

pub(crate) fn check(directory: PathBuf) -> Result<(), Box<dyn Error>> {
    let project = match DogwoodProject::discover(&directory) {
        Ok(project) => project,
        Err(error) => {
            println!("Dogwood project: no ({error})");
            return Ok(());
        }
    };
    let host = BuildTarget::host();
    let cargo = command_available("cargo", &["--version"]);
    let docker = command_available("docker", &["--version"]);
    let daemon = docker && command_available("docker", &["info"]);
    let buildx = daemon && command_available("docker", &["buildx", "version"]);
    println!(
        "Dogwood project:      yes\nWorkspace:            {}\nGame package:         {}\nGame binary:          {}\nHost target:          {}\nCargo:                {}\nDocker:               {}\nDocker daemon:        {}\nDocker BuildKit:      {}\nWindows cross-build:  {}\nLinux cross-build:    {}\nBuild files:          {}",
        project.root.display(),
        project.game_package,
        project.game_binary,
        host.as_ref()
            .map_or("unsupported".to_owned(), ToString::to_string),
        yes_no(cargo),
        yes_no(docker),
        yes_no(daemon),
        yes_no(buildx),
        yes_no(buildx),
        yes_no(buildx),
        yes_no(crate::build::infrastructure_present())
    );
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value { "available" } else { "unavailable" }
}
