// Copyright Rob Gage 2026

use std::{env, fs, path::PathBuf};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    let target = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "x86_64" => "windows-x64",
        "aarch64" => "windows-arm64",
        arch => panic!("No bundled DXC for Windows architecture {arch}"),
    };
    let source = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("../third_party/dxc").join(target).join("dxcompiler.dll");
    println!("cargo:rerun-if-changed={}", source.display());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let profile = out.ancestors().nth(3).expect("Cargo profile directory");
    fs::create_dir_all(profile.join("deps")).unwrap();
    fs::copy(&source, profile.join("dxcompiler.dll")).unwrap();
    fs::copy(&source, profile.join("deps/dxcompiler.dll")).unwrap();
}
