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
    #[arg(long, group = "platform")]
    pub(crate) windows: bool,
    #[arg(long, group = "platform")]
    pub(crate) linux: bool,
    #[arg(long, group = "architecture")]
    pub(crate) x86: bool,
    #[arg(long, group = "architecture")]
    pub(crate) x64: bool,
    #[arg(long, group = "architecture")]
    pub(crate) arm64: bool,
}

impl TargetFlags {
    pub(crate) fn target(&self) -> Result<BuildTarget, Box<dyn Error>> {
        let platform_count = self.windows as u8 + self.linux as u8;
        let architecture_count = self.x86 as u8 + self.x64 as u8 + self.arm64 as u8;
        if platform_count == 0 && architecture_count == 0 {
            return BuildTarget::host().map_err(Into::into);
        }
        if platform_count != 1 || architecture_count != 1 {
            return Err("choose exactly one platform (--windows or --linux) and exactly one architecture (--x86, --x64, or --arm64), or omit all target flags".into());
        }
        let platform = if self.windows { "windows" } else { "linux" };
        let architecture = if self.x86 {
            "x86"
        } else if self.x64 {
            "x64"
        } else {
            "arm64"
        };
        format!("{platform}-{architecture}")
            .parse()
            .map_err(Into::into)
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

#[cfg(test)]
mod tests {
    use super::TargetFlags;
    use crate::target::BuildTarget;

    #[test]
    fn target_flags_require_a_complete_pair() {
        assert!(
            TargetFlags {
                windows: true,
                ..Default::default()
            }
            .target()
            .is_err()
        );
        assert!(
            TargetFlags {
                x64: true,
                ..Default::default()
            }
            .target()
            .is_err()
        );
        assert_eq!(
            TargetFlags {
                windows: true,
                x64: true,
                ..Default::default()
            }
            .target()
            .unwrap(),
            BuildTarget::WindowsX64
        );
    }

    #[test]
    fn no_target_flags_select_the_host() {
        assert_eq!(
            TargetFlags::default().target().unwrap(),
            BuildTarget::host().unwrap()
        );
    }
}
