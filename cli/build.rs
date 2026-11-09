use std::{fs, path::Path};
fn copy(source: &Path, destination: &Path) {
    println!("cargo:rerun-if-changed={}", source.display());
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &destination.join(entry.file_name()));
        } else if path.extension().is_some_and(|e| e == "wgsl") {
            println!("cargo:rerun-if-changed={}", path.display());
            fs::copy(path, destination.join(entry.file_name())).unwrap();
        }
    }
}
fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("shaders");
    if out.exists() {
        fs::remove_dir_all(&out).unwrap();
    }
    for name in ["engine_graphics", "engine_world"] {
        copy(
            &Path::new("..").join(name).join("src"),
            &out.join(name).join("src"),
        );
    }
}
