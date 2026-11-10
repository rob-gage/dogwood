use engine_resources::{Catalogue, Result};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    name: Option<String>,
    binary: Option<String>,
    bundle_identifier: Option<String>,
    linux_glibc: Option<String>,
}
fn error(s: impl Into<String>) -> std::io::Error {
    std::io::Error::other(s.into())
}
/// Query RUSTC or the compiler on PATH, including distro toolchains without rustup.
fn require_target_standard_library(triple: &str) -> Result<()> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(&rustc)
        .args(["--print", "target-libdir", "--target", triple])
        .output()
        .map_err(|e| error(format!("could not run Rust compiler {rustc:?}: {e}; install Rust or set RUSTC to its executable")))?;
    let library_dir = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let libraries: Vec<_> = fs::read_dir(&library_dir)
        .into_iter().flatten().flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    if !output.status.success() || !["libstd-", "libcore-"].iter().all(|prefix| {
        libraries.iter().any(|name| name.starts_with(prefix) && name.ends_with(".rlib"))
    }) {
        return Err(error(format!(
            "missing Rust standard library for {triple}; install your compiler's target libraries or run: rustup target add {triple}. {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )).into());
    }
    Ok(())
}
fn run(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if !status.success() {
        return Err(error(format!("command failed ({status}): {command:?}")).into());
    }
    Ok(())
}
/// Validate assembled shader modules; WGSL remains the safe runtime wgpu input.
static ENGINE_SHADERS: include_dir::Dir = include_dir::include_dir!("$OUT_DIR/shaders");
fn shaders(metal: bool, builtin: Option<&str>, catalogue: Option<&Catalogue>) -> Result<()> {
    let read = |p: &str| -> Result<String> {
        ENGINE_SHADERS
            .get_file(p)
            .and_then(|f| f.contents_utf8())
            .map(String::from)
            .ok_or_else(|| error(format!("missing embedded shader {p}")).into())
    };
    let radiance = read("engine_graphics/src/world/materials/material_radiance.wgsl")?;
    let lighting = read("engine_graphics/src/renders/scene_renderer_lighting.wgsl")?;
    let shaded = read("engine_graphics/src/renders/scene_renderer_shaded.wgsl")?;
    fn collect(dir: &include_dir::Dir, entries: &mut Vec<(PathBuf, String)>) {
        for file in dir.files() {
            entries.push((
                file.path().to_owned(),
                String::from_utf8_lossy(file.contents()).into_owned(),
            ));
        }
        for child in dir.dirs() {
            collect(child, entries);
        }
    }
    let mut entries = Vec::new();
    if let Some(name) = builtin {
        collect(
            ENGINE_SHADERS
                .get_dir(name)
                .ok_or_else(|| error("missing embedded shader directory"))?,
            &mut entries,
        );
    } else {
        let catalogue = catalogue.ok_or_else(|| error("missing game shader catalogue"))?;
        for entry in &catalogue.entries {
            if Path::new(&entry.path).extension().and_then(|s| s.to_str()) == Some("wgsl") {
                let path = catalogue.root.join(&entry.path);
                entries.push((path.clone(), fs::read_to_string(path)?));
            }
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, mut source) in entries {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| error("non UTF-8 shader filename"))?;
        // These fragments are checked as part of their consuming module below. Reference shaders
        // belong to test harnesses and are not shipped or consumed by runtime pipelines.
        if matches!(
            name,
            "material_radiance.wgsl"
                | "scene_renderer_lighting.wgsl"
                | "scene_renderer_shaded.wgsl"
                | "thermal_transition.wgsl"
                | "thermal_conversion.wgsl"
                | "cellular_tile_fluids.wgsl"
                | "chemistry_contact_reference.wgsl"
        ) {
            continue;
        }
        if source.contains("fn shade(input: ShadeInput)") {
            source = format!("{shaded}\n{source}");
        }
        let lit = matches!(
            name,
            "scene_renderer_sprites.wgsl"
                | "scene_renderer_gas.wgsl"
                | "scene_renderer_decorations.wgsl"
                | "scene_renderer_cellular.wgsl"
                | "scene_renderer_fluid.wgsl"
        ) || source.contains("sample_lighting(")
            || source.contains("LightingView")
            || source.contains("lighting_view.")
            || name == "scene_renderer_lighting_optical.wgsl";
        if lit || source.contains("ShadeInput") {
            let group = if matches!(
                name,
                "scene_renderer_sprites.wgsl" | "scene_renderer_decorations.wgsl"
            ) {
                2
            } else {
                1
            };
            source =
                format!("{radiance}\nconst LIGHTING_GROUP: u32 = {group}u;\n{lighting}\n{source}");
        } else if matches!(
            name,
            "chemistry_cellular_tile.wgsl" | "thermal_cellular_tile.wgsl" | "fluid_particle.wgsl"
        ) {
            source = format!(
                "{radiance}\n{source}\n{}",
                read("engine_world/src/scenes/thermal/thermal_transition.wgsl")?
            );
            if name != "fluid_particle.wgsl" {
                source.push_str(&read(
                    "engine_world/src/scenes/thermal/thermal_conversion.wgsl",
                )?);
            }
        } else if name == "cellular_tile.wgsl" {
            source.push_str(&read(
                "engine_world/src/scenes/cellular_tiles/cellular_tile_fluids.wgsl",
            )?);
        }
        // Fluid material offsets and device alignment are runtime values. Validate representative
        // legal substitutions; the actual module still receives the runtime table and limits.
        for (key, value) in [
            ("__FLUID_BIN_SIZE__", "2.0"),
            ("__FLUID_POSITION_RELAXATION__", "0.1"),
            ("__THERMAL_OFFSETS__", "vec4<u32>(0u, 1u, 2u, 3u)"),
            ("__THERMAL_HEAT_SCALE__", "1.0"),
            ("__THERMAL_FLUID_CAPACITY_SCALE__", "1.0"),
            ("__THERMAL_FLUID_ENERGY_SCALE__", "1.0"),
            ("__THERMAL_FLUID_CONDUCTION_SCALE__", "1.0"),
            ("__FLUID_RASTER_ALIGNMENT_WORDS__", "64"),
        ] {
            source = source.replace(key, value);
        }
        let module = naga::front::wgsl::parse_str(&source)
            .map_err(|e| error(format!("{}: {}", path.display(), e.emit_to_string(&source))))?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|e| error(format!("{}: {e:?}", path.display())))?;
        // Resolve pipeline override defaults for translation; wgpu applies actual pipeline constants.
        let (module, info) = naga::back::pipeline_constants::process_overrides(
            &module,
            &info,
            None,
            &Default::default(),
        )?;
        if metal {
            let (_, translation) = naga::back::msl::write_string(
                &module,
                &info,
                &naga::back::msl::Options {
                    lang_version: (2, 4),
                    fake_missing_bindings: true,
                    ..Default::default()
                },
                &naga::back::msl::PipelineOptions::default(),
            )
            .map_err(|e| error(format!("{}: Metal translation: {e}", path.display())))?;
            for ep in translation.entry_point_names {
                ep.map_err(|e| error(format!("{}: {e:?}", path.display())))?;
            }
        } else {
            naga::back::spv::write_vec(&module, &info, &naga::back::spv::Options::default(), None)?;
        }
    }
    Ok(())
}
fn mac_bundle(
    stage: &Path,
    name: &str,
    bundle_identifier: Option<String>,
    version: &str,
) -> Result<(PathBuf, PathBuf)> {
    let contents = stage.join(format!("{name}.app/Contents"));
    let mac = contents.join("MacOS");
    let res = contents.join("Resources");
    fs::create_dir_all(&mac)?;
    fs::create_dir_all(&res)?;
    let id = bundle_identifier.unwrap_or_else(|| format!("games.dogwood.{name}"));
    fn xml(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    fs::write(
        contents.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleExecutable</key><string>{}</string><key>CFBundleName</key><string>{}</string><key>CFBundleIdentifier</key><string>{}</string><key>CFBundleVersion</key><string>{}</string><key>LSApplicationCategoryType</key><string>public.app-category.games</string><key>NSHighResolutionCapable</key><true/><key>LSSupportsGameMode</key><true/></dict></plist>"#,
            xml(&name),
            xml(&name),
            xml(&id),
            xml(version)
        ),
    )?;
    Ok((mac, res))
}
/// Explicit output replaces the distribution root and owns its Cargo intermediates.
fn build_directories(project: &Path, cargo_target: &Path, output: Option<&Path>) -> (PathBuf, PathBuf) {
    match output {
        Some(output) => {
            let root = if output.is_absolute() { output.to_owned() } else { project.join(output) };
            (root.join("intermediate"), root)
        }
        None => (cargo_target.to_owned(), project.join("dist")),
    }
}

pub fn distribute(
    cargo: PathBuf,
    path: PathBuf,
    debug: bool,
    target: Option<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    let path = path.canonicalize()?;
    let target = target.unwrap_or_else(|| {
        if cfg!(target_os = "windows") {
            if cfg!(target_arch = "aarch64") { "windows-arm64" } else { "windows-x64" }
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "macos-arm64"
            } else {
                "macos-x64"
            }
        } else {
            "linux-x64"
        }
        .into()
    });
    let (target, triple, metal) = match target.as_str() {
        "linux" | "linux-x64" | "x86_64-unknown-linux-gnu" => {
            ("linux-x64", "x86_64-unknown-linux-gnu", false)
        }
        "windows-arm64" | "aarch64-pc-windows-msvc" => {
            ("windows-arm64", "aarch64-pc-windows-msvc", false)
        }
        "windows" | "windows-x64" | "x86_64-pc-windows-msvc" => {
            ("windows-x64", "x86_64-pc-windows-msvc", false)
        }
        "macos" | "macos-arm64" | "aarch64-apple-darwin" => {
            ("macos-arm64", "aarch64-apple-darwin", true)
        }
        "macos-x64" | "x86_64-apple-darwin" => ("macos-x64", "x86_64-apple-darwin", true),
        _ => return Err(error(format!("unsupported target: {target}")).into()),
    };
    let config: Config = if path.join("dogwood.toml").exists() {
        toml::from_str(&fs::read_to_string(path.join("dogwood.toml"))?)?
    } else {
        Config::default()
    };
    let metadata = Command::new(&cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(&path)
        .output()?;
    if !metadata.status.success() {
        return Err(error(String::from_utf8_lossy(&metadata.stderr)).into());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout)?;
    let package = metadata["packages"]
        .as_array()
        .and_then(|a| {
            a.iter().find(|p| {
                p["manifest_path"].as_str().map(Path::new)
                    == Some(path.join("Cargo.toml").as_path())
            })
        })
        .ok_or_else(|| error("run build in a game package, not a virtual workspace"))?;
    let binary = config
        .binary
        .or_else(|| package["default_run"].as_str().map(String::from))
        .or_else(|| {
            let bins: Vec<_> = package["targets"]
                .as_array()?
                .iter()
                .filter(|t| {
                    t["kind"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|k| k == "bin"))
                })
                .collect();
            if bins.len() == 1 {
                bins[0]["name"].as_str().map(String::from)
            } else {
                None
            }
        })
        .ok_or_else(|| error("set binary in dogwood.toml when Cargo has multiple binaries"))?;
    let name = config.name.unwrap_or_else(|| binary.clone());
    if name.is_empty()
        || name
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
    {
        return Err(
            error("distribution name must contain only ASCII letters, digits, '-' or '_'").into(),
        );
    }
    let catalogue = Catalogue::read(path.join("assets"), !debug)?;
    shaders(metal, None, Some(&catalogue))?;
    // Embedded engine shaders remain WGSL; validate their native translation before Cargo.
    shaders(metal, Some("engine_graphics"), None)?;
    shaders(metal, Some("engine_world"), None)?;
    if metal && !cfg!(target_os = "macos") {
        return Err(error("macOS compilation requires a Mac with Xcode Command Line Tools: xcode-select --install; rustup target add aarch64-apple-darwin x86_64-apple-darwin").into());
    }
    require_target_standard_library(triple)?;
    let (target_dir, distribution_root) = build_directories(
        &path,
        Path::new(metadata["target_directory"].as_str().unwrap()),
        output.as_deref(),
    );
    let mut build = Command::new(&cargo);
    if target.starts_with("windows-") && !cfg!(target_os = "windows") {
        if which::which("cargo-xwin").is_err()
            || which::which("clang-cl").is_err()
            || which::which("lld-link").is_err()
            || which::which("llvm-lib").is_err()
        {
            return Err(error("Windows MSVC cross-build requires cargo-xwin, clang-cl, lld-link and llvm-lib. Install LLVM/Clang/lld and run: cargo install cargo-xwin --locked; rustup target add x86_64-pc-windows-msvc. cargo-xwin downloads the Microsoft CRT/SDK (license acceptance required).").into());
        }
        build.args(["xwin", "build"]);
    } else if target == "linux-x64" && config.linux_glibc.is_some() {
        if which::which("cargo-zigbuild").is_err() || which::which("zig").is_err() {
            return Err(error("linux_glibc requires cargo-zigbuild and Zig on PATH: cargo install cargo-zigbuild --locked; install Zig 0.14.1").into());
        }
        build.arg("zigbuild");
        build.env("PKG_CONFIG_ALLOW_CROSS", "1");
    } else {
        build.arg("build");
    }
    let build_triple = if target == "linux-x64" {
        if let Some(version) = &config.linux_glibc {
            if version.split('.').count() != 2
                || !version
                    .split('.')
                    .all(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_digit()))
            {
                return Err(error("linux_glibc must be a major.minor version, e.g. 2.28").into());
            }
            format!("{triple}.{version}")
        } else {
            triple.into()
        }
    } else {
        triple.into()
    };
    build
        .current_dir(&path)
        .args(["--bin", &binary, "--target", &build_triple, "--target-dir"])
        .arg(&target_dir);
    if !debug {
        build.arg("--release");
    }
    if target.starts_with("windows-") {
        let flags = std::env::var("RUSTFLAGS").unwrap_or_default();
        build.env(
            "RUSTFLAGS",
            format!("{flags} -C target-feature=+crt-static"),
        );
    }
    run(&mut build)?;
    // Stage outside dist. A failed build never replaces an existing usable distribution.
    let stage = target_dir.join("dogwood-stage").join(target).join(&name);
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    fs::create_dir_all(&stage)?;
    let (exe_dir, data_dir) = if metal {
        mac_bundle(
            &stage,
            &name,
            config.bundle_identifier,
            package["version"].as_str().unwrap_or("1"),
        )?
    } else {
        (stage.clone(), stage.clone())
    };
    let suffix = if target.starts_with("windows-") { ".exe" } else { "" };
    fs::copy(
        target_dir
            .join(triple)
            .join(if debug { "debug" } else { "release" })
            .join(format!("{binary}{suffix}")),
        exe_dir.join(format!("{name}{suffix}")),
    )?;
    if debug {
        catalogue.copy_loose(&data_dir.join("assets"))?;
    } else {
        catalogue.pack(&data_dir.join("resources"))?;
    }
    super::windows_vulkan::package(target, &exe_dir)?;
    let destination = distribution_root.join(target).join(&name);
    fs::create_dir_all(destination.parent().unwrap())?;
    let backup = destination.with_extension("previous");
    if backup.exists() {
        return Err(error(format!(
            "preserved previous distribution exists: {}",
            backup.display()
        ))
        .into());
    }
    if destination.exists() {
        fs::rename(&destination, &backup)?;
    }
    if let Err(e) = fs::rename(&stage, &destination) {
        if backup.exists() {
            fs::rename(&backup, &destination)?;
        }
        return Err(e.into());
    }
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    println!("Distribution: {}", destination.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_selects_distribution_root_with_intermediate_subdirectory() {
        let project = std::env::temp_dir().join("dogwood-project");
        let cargo = project.join("target");
        assert_eq!(build_directories(&project, &cargo, None), (cargo.clone(), project.join("dist")));
        let relative = Path::new("builds");
        assert_eq!(build_directories(&project, &cargo, Some(relative)),
            (project.join("builds/intermediate"), project.join("builds")));
        let absolute = std::env::temp_dir().join("dogwood-output");
        assert_eq!(build_directories(&project, &cargo, Some(&absolute)),
            (absolute.join("intermediate"), absolute));
    }


    #[test]
    fn target_check_accepts_the_active_compiler_without_requiring_rustup() {
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let output = Command::new(rustc).arg("-vV").output().unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        let host = text.lines().find_map(|line| line.strip_prefix("host: ")).unwrap();
        require_target_standard_library(host).unwrap();
    }
    #[test]
    fn missing_target_reports_the_target_and_installation_remedy() {
        let message = require_target_standard_library("unknown-dogwood-target").unwrap_err().to_string();
        assert!(message.contains("unknown-dogwood-target"));
        assert!(message.contains("standard library"));
        assert!(message.contains("rustup target add"));
    }
    #[test]
    fn engine_shaders_translate_to_both_native_backends() {
        for metal in [false, true] {
            for name in ["engine_graphics", "engine_world"] {
                shaders(metal, Some(name), None).unwrap();
            }
        }
    }
    #[test]
    fn native_bundle_layout_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let (exe, data) = mac_bundle(
            dir.path(),
            "Fixture",
            Some("games.example.fixture".into()),
            "1.2.3",
        )
        .unwrap();
        assert!(exe.ends_with("Fixture.app/Contents/MacOS") && exe.is_dir());
        assert!(data.ends_with("Fixture.app/Contents/Resources") && data.is_dir());
        let plist = fs::read_to_string(dir.path().join("Fixture.app/Contents/Info.plist")).unwrap();
        for value in [
            "games.example.fixture",
            "LSSupportsGameMode",
            "CFBundleExecutable",
            "1.2.3",
            "APPL",
        ] {
            assert!(plist.contains(value));
        }
    }
}
