// Copyright Rob Gage 2026

use crate::{project::DogwoodProject, target::BuildTarget};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub(crate) struct BuildArtifact {
    pub(crate) package_dir: PathBuf,
    pub(crate) executable: PathBuf,
}

pub(crate) fn build(
    project: &DogwoodProject,
    target: BuildTarget,
) -> Result<BuildArtifact, Box<dyn Error>> {
    build_to(
        project,
        target,
        project.root.join("dist").join(target.name()),
    )
}

pub(crate) fn build_to(
    project: &DogwoodProject,
    target: BuildTarget,
    destination: PathBuf,
) -> Result<BuildArtifact, Box<dyn Error>> {
    fs::create_dir_all(&destination)?;
    for filename in ["game", "game.exe", "vulkan-1.dll", "libvulkan.so.1"] {
        let path = destination.join(filename);
        if path.is_file() {
            fs::remove_file(path)?;
        }
    }
    let dockerfile = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../build/Dockerfile");
    let dogwood_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()?;
    let output = format!("type=local,dest={}", destination.display());
    let mut command = Command::new("docker");
    command
        .args(["buildx", "build", "--file"])
        .arg(&dockerfile)
        .arg("--build-context")
        .arg(format!("dogwood_engine={}", dogwood_root.display()))
        .arg("--build-arg")
        .arg(format!("DOGWOOD_TARGET={}", target.name()))
        .arg("--build-arg")
        .arg(format!("RUST_TARGET={}", target.triple()))
        .arg("--build-arg")
        .arg(format!("GAME_PACKAGE={}", project.game_package))
        .arg("--build-arg")
        .arg(format!("GAME_BINARY={}", project.game_binary))
        .arg("--build-arg")
        .arg(format!(
            "DOGWOOD_RUNTIME_ASSET={}",
            target
                .runtime_asset()
                .map_or("".to_owned(), |asset| asset.source.to_owned())
        ))
        .arg("--output")
        .arg(&output)
        .arg("--tag")
        .arg(format!("dogwood-{}", target.name()))
        .args(["--progress", "plain"])
        .arg(&project.root)
        .stdin(Stdio::null());
    let status = command.status().map_err(|error| {
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
    let artifact = BuildArtifact {
        package_dir: destination.clone(),
        executable: destination.join(target.executable_name()),
    };
    validate_artifact(&artifact, target)?;
    Ok(artifact)
}

pub(crate) fn validate_artifact(
    artifact: &BuildArtifact,
    target: BuildTarget,
) -> Result<(), Box<dyn Error>> {
    if !artifact.executable.is_file() {
        return Err(format!("built package is missing {}", artifact.executable.display()).into());
    }
    if let Some(asset) = target.runtime_asset() {
        let loader = artifact.package_dir.join(asset.filename);
        if !loader.is_file() {
            return Err(format!(
                "built package is missing packaged Vulkan loader {}",
                loader.display()
            )
            .into());
        }
    }
    Ok(())
}

pub(crate) fn run_cache_directory(
    project: &DogwoodProject,
    target: BuildTarget,
) -> Result<PathBuf, Box<dyn Error>> {
    let project_id = short_hash(project.root.to_string_lossy().as_bytes());
    Ok(std::env::temp_dir()
        .join("dogwood-run")
        .join(project_id)
        .join(target.name()))
}

pub(crate) fn fingerprint(
    project: &DogwoodProject,
    target: BuildTarget,
) -> Result<String, Box<dyn Error>> {
    let dogwood_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()?;
    let mut hash = Sha256::new();
    hash.update(b"dogwood-run-fingerprint-v1\0");
    hash.update(target.name().as_bytes());
    hash.update(b"\0release\0");
    hash.update(env!("CARGO_PKG_VERSION").as_bytes());
    hash_tree(&mut hash, &project.root, "project")?;
    if dogwood_root != project.root {
        hash_tree(&mut hash, &dogwood_root, "dogwood")?;
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn hash_tree(hash: &mut Sha256, root: &Path, label: &str) -> Result<(), Box<dyn Error>> {
    let mut entries = Vec::new();
    collect_files(root, &mut entries)?;
    entries.sort();
    for path in entries {
        hash.update(label.as_bytes());
        hash.update(b"/");
        hash.update(path.strip_prefix(root)?.to_string_lossy().as_bytes());
        hash.update(b"\0");
        hash.update(fs::read(path)?);
    }
    Ok(())
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    let mut entries: Vec<_> = fs::read_dir(directory)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if matches!(
                entry.file_name().to_str(),
                Some(".git" | "target" | "dist" | "dogwood-run" | ".dogwood-run")
            ) {
                continue;
            }
            collect_files(&path, files)?;
        } else if entry.file_type()?.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn short_hash(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    format!("{:x}", hash.finalize())[..16].to_owned()
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

#[cfg(test)]
mod tests {
    use super::{fingerprint, short_hash};
    use crate::{project::DogwoodProject, target::BuildTarget};
    use std::{fs, path::PathBuf};

    #[test]
    fn project_cache_ids_are_stable_and_short() {
        assert_eq!(short_hash(b"project"), short_hash(b"project"));
        assert_eq!(short_hash(b"project").len(), 16);
        assert_ne!(short_hash(b"project"), short_hash(b"other"));
    }

    #[test]
    fn fingerprint_changes_for_source_and_target_but_ignores_generated_files() {
        let root =
            std::env::temp_dir().join(format!("dogwood-fingerprint-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let project = DogwoodProject {
            root: root.clone(),
            game_package: "game".into(),
            game_binary: "game".into(),
        };
        let original = fingerprint(&project, BuildTarget::LinuxX64).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/generated"), "ignored").unwrap();
        fs::create_dir_all(root.join("dist")).unwrap();
        fs::write(root.join("dist/generated"), "ignored").unwrap();
        assert_eq!(
            original,
            fingerprint(&project, BuildTarget::LinuxX64).unwrap()
        );
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"changed\"); }\n",
        )
        .unwrap();
        assert_ne!(
            original,
            fingerprint(&project, BuildTarget::LinuxX64).unwrap()
        );
        assert_ne!(
            original,
            fingerprint(&project, BuildTarget::LinuxX86).unwrap()
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn package_validation_requires_executable_and_windows_loader() {
        let root = PathBuf::from(std::env::temp_dir())
            .join(format!("dogwood-package-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("game.exe"), []).unwrap();
        let artifact = super::BuildArtifact {
            package_dir: root.clone(),
            executable: root.join("game.exe"),
        };
        assert!(super::validate_artifact(&artifact, BuildTarget::WindowsX64).is_err());
        fs::write(root.join("vulkan-1.dll"), []).unwrap();
        assert!(super::validate_artifact(&artifact, BuildTarget::WindowsX64).is_ok());
        let _ = fs::remove_dir_all(&root);
    }
}
