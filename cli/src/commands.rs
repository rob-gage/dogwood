// Copyright Rob Gage 2026

use crate::{
    build::{build, command_available},
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
    println!("Built {target}: {}", artifact.executable.display());
    Ok(())
}

pub(crate) fn run(directory: PathBuf, flags: TargetFlags) -> Result<(), Box<dyn Error>> {
    let project = DogwoodProject::discover(&directory)?;
    let target = flags.target()?;
    let artifact = build(&project, target)?;
    if target != BuildTarget::host()? {
        println!("Built {target}: {}", artifact.executable.display());
        return Err(format!(
            "cannot execute {target} natively on this host; artifact is at {}",
            artifact.executable.display()
        )
        .into());
    }
    let status = Command::new(&artifact.executable)
        .current_dir(&project.root)
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
