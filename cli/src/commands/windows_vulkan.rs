// Copyright Rob Gage 2026

use std::{fs, path::Path};

/// The updater validates published archive checksums and architecture before vendoring.
const X64: &[u8] = include_bytes!("../../../third_party/vulkan/windows-x64/vulkan-1.dll");
const ARM64: &[u8] = include_bytes!("../../../third_party/vulkan/windows-arm64/vulkan-1.dll");
const X64_LICENSE: &[u8] = include_bytes!("../../../third_party/vulkan/windows-x64/VulkanRT-License.txt");
const ARM64_LICENSE: &[u8] = include_bytes!("../../../third_party/vulkan/windows-arm64/VulkanRT-License.txt");
const MANIFEST: &str = include_str!("../../../third_party/vulkan/manifest.json");

pub(super) fn package(target: &str, executable_directory: &Path) -> std::io::Result<()> {
    let (dll, license) = match target {
        "windows-x64" => (X64, X64_LICENSE),
        "windows-arm64" => (ARM64, ARM64_LICENSE),
        _ => return Ok(()),
    };
    let directory = executable_directory.join("vulkan");
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("vulkan-1.dll"), dll)?;
    fs::write(directory.join("VulkanRT-License.txt"), license)?;
    fs::write(directory.join("manifest.json"), MANIFEST)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_matching_pe_architecture_and_license_outside_default_dll_search() {
        for (target, machine) in [("windows-x64", 0x8664_u16), ("windows-arm64", 0xaa64)] {
            let directory = tempfile::tempdir().unwrap();
            package(target, directory.path()).unwrap();
            assert!(!directory.path().join("vulkan-1.dll").exists());
            let dll = fs::read(directory.path().join("vulkan/vulkan-1.dll")).unwrap();
            assert_eq!(&dll[..2], b"MZ");
            let offset = u32::from_le_bytes(dll[0x3c..0x40].try_into().unwrap()) as usize;
            assert_eq!(&dll[offset..offset + 4], b"PE\0\0");
            assert_eq!(u16::from_le_bytes(dll[offset + 4..offset + 6].try_into().unwrap()), machine);
            assert!(!fs::read(directory.path().join("vulkan/VulkanRT-License.txt")).unwrap().is_empty());
            let manifest: serde_json::Value = serde_json::from_str(MANIFEST).unwrap();
            assert!(manifest["loaders"][target]["url"].as_str().unwrap().starts_with("https://sdk.lunarg.com/"));
        }
    }

    #[test]
    fn other_platforms_do_not_bundle_windows_dlls() {
        let directory = tempfile::tempdir().unwrap();
        package("linux-x64", directory.path()).unwrap();
        assert!(!directory.path().join("vulkan").exists());
    }
}
