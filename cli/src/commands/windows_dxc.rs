// Copyright Rob Gage 2026

use std::{fs, path::Path};

const X64: &[u8] = include_bytes!("../../../third_party/dxc/windows-x64/dxcompiler.dll");
const ARM64: &[u8] = include_bytes!("../../../third_party/dxc/windows-arm64/dxcompiler.dll");
const LLVM: &[u8] = include_bytes!("../../../third_party/dxc/LICENSE-LLVM.txt");
const MIT: &[u8] = include_bytes!("../../../third_party/dxc/LICENSE-MIT.txt");
const NOTES: &[u8] = include_bytes!("../../../third_party/dxc/ReleaseNotes.md");
const MANIFEST: &str = include_str!("../../../third_party/dxc/manifest.json");

pub(super) fn package(target: &str, executable_directory: &Path) -> std::io::Result<()> {
    let dll = match target {
        "windows-x64" => X64,
        "windows-arm64" => ARM64,
        _ => return Ok(()),
    };
    fs::write(executable_directory.join("dxcompiler.dll"), dll)?;
    let directory = executable_directory.join("licenses/dxc");
    fs::create_dir_all(&directory)?;
    for (name, contents) in [("LICENSE-LLVM.txt", LLVM), ("LICENSE-MIT.txt", MIT), ("ReleaseNotes.md", NOTES), ("manifest.json", MANIFEST.as_bytes())] {
        fs::write(directory.join(name), contents)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_matching_dxc_architecture_and_notices_without_vulkan() {
        for (target, machine) in [("windows-x64", 0x8664_u16), ("windows-arm64", 0xaa64)] {
            let directory = tempfile::tempdir().unwrap();
            package(target, directory.path()).unwrap();
            let dll = fs::read(directory.path().join("dxcompiler.dll")).unwrap();
            assert_eq!(&dll[..2], b"MZ");
            let offset = u32::from_le_bytes(dll[0x3c..0x40].try_into().unwrap()) as usize;
            assert_eq!(&dll[offset..offset + 4], b"PE\0\0");
            assert_eq!(u16::from_le_bytes(dll[offset + 4..offset + 6].try_into().unwrap()), machine);
            assert!(!directory.path().join("vulkan").exists());
            assert!(!fs::read(directory.path().join("licenses/dxc/LICENSE-LLVM.txt")).unwrap().is_empty());
            let manifest: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
            assert_eq!(manifest["version"], "1.8.2505.1");
        }
    }

    #[test]
    fn other_platforms_do_not_bundle_windows_dlls() {
        let directory = tempfile::tempdir().unwrap();
        package("linux-x64", directory.path()).unwrap();
        package("macos-arm64", directory.path()).unwrap();
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
