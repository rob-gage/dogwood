// Copyright Rob Gage 2026

use sha2::{Digest, Sha256};
use std::{
    error::Error,
    ffi::OsStr,
    fs,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32,
        LoadLibraryExW,
    },
};

const LOADER_VERSION: &str = "1.4.357.0";
const LOADER_FILENAME: &str = "vulkan-1.dll";

#[cfg(feature = "windows-vulkan-loader-x86")]
const LOADER_ARCHITECTURE: &str = "x86";
#[cfg(feature = "windows-vulkan-loader-x86")]
const LOADER_BYTES: &[u8] = include_bytes!("../runtime/windows/vulkan/x86/vulkan-1.dll");
#[cfg(feature = "windows-vulkan-loader-x86")]
const LOADER_DIGEST: [u8; 32] = [
    0xb1, 0xca, 0x65, 0xb9, 0x32, 0x1a, 0xd4, 0xe7, 0x62, 0x56, 0xfd, 0x72, 0xf9, 0xcb, 0x56, 0x10,
    0xff, 0xda, 0x1c, 0xf2, 0xa9, 0x57, 0xf0, 0x7d, 0x7b, 0x77, 0xc8, 0xb1, 0x22, 0x72,
];

#[cfg(feature = "windows-vulkan-loader-x64")]
const LOADER_ARCHITECTURE: &str = "x64";
#[cfg(feature = "windows-vulkan-loader-x64")]
const LOADER_BYTES: &[u8] = include_bytes!("../runtime/windows/vulkan/x64/vulkan-1.dll");
#[cfg(feature = "windows-vulkan-loader-x64")]
const LOADER_DIGEST: [u8; 32] = [
    0xcd, 0x86, 0x20, 0x93, 0x70, 0x45, 0x46, 0x30, 0xb3, 0x1b, 0x17, 0x4e, 0x3d, 0x4e, 0xb4, 0x47,
    0x4f, 0xda, 0x38, 0xea, 0x03, 0x49, 0x98, 0xd1, 0xfe, 0x17, 0x6b, 0xb0, 0xc9, 0x9a, 0x86, 0x96,
];

#[cfg(feature = "windows-vulkan-loader-arm64")]
const LOADER_ARCHITECTURE: &str = "arm64";
#[cfg(feature = "windows-vulkan-loader-arm64")]
const LOADER_BYTES: &[u8] = include_bytes!("../runtime/windows/vulkan/arm64/vulkan-1.dll");
#[cfg(feature = "windows-vulkan-loader-arm64")]
const LOADER_DIGEST: [u8; 32] = [
    0xcc, 0x5d, 0xd0, 0xbe, 0xc8, 0xa7, 0xaf, 0xef, 0x01, 0x3c, 0x61, 0xdd, 0xd5, 0x11, 0xd3, 0x15,
    0x00, 0xa3, 0xb3, 0xa4, 0x5c, 0x22, 0x90, 0x15, 0x0a, 0x9a, 0xe7, 0x3e, 0xad, 0x70, 0x8a, 0xb7,
];

pub(crate) struct LoadedVulkanLoader {
    path: PathBuf,
    module: HMODULE,
}

impl LoadedVulkanLoader {
    pub(crate) fn load() -> Result<Self, Box<dyn Error>> {
        let expected_digest = digest(LOADER_BYTES);
        if expected_digest != LOADER_DIGEST {
            return Err(format!(
                "embedded Vulkan loader digest mismatch for {LOADER_ARCHITECTURE}"
            )
            .into());
        }
        let (base, mut used_fallback) = runtime_base_directory();
        let (path, reused) = match materialize(&cache_path(&base)) {
            Ok(result) => (cache_path(&base), result),
            Err(local_error) if !used_fallback => {
                let fallback = temporary_base_directory();
                tracing::warn!(
                    target: "dogwood_accelerator",
                    error = %local_error,
                    path = %base.display(),
                    "unable to materialize Vulkan loader in LocalAppData; trying temporary directory"
                );
                used_fallback = true;
                let fallback_path = cache_path(&fallback);
                (fallback_path.clone(), materialize(&fallback_path).map_err(|fallback_error| {
                    format!(
                        "LocalAppData Vulkan loader failed: {local_error}; temporary fallback failed: {fallback_error}"
                    )
                })?)
            }
            Err(error) => return Err(error),
        };
        let absolute_path = fs::canonicalize(&path).map_err(|error| {
            format!(
                "unable to resolve embedded Vulkan loader path {}: {error}",
                path.display()
            )
        })?;
        tracing::info!(
            target: "dogwood_accelerator",
            architecture = LOADER_ARCHITECTURE,
            version = LOADER_VERSION,
            hash = %digest_hex(&LOADER_DIGEST),
            path = %absolute_path.display(),
            reused,
            used_fallback,
            "embedded Vulkan loader ready"
        );
        let wide_path: Vec<u16> = OsStr::new(&absolute_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let module = unsafe {
            LoadLibraryExW(
                wide_path.as_ptr(),
                ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if module.is_null() {
            return Err(format!(
                "LoadLibraryExW failed for {} (Windows error {})",
                absolute_path.display(),
                unsafe { windows_sys::Win32::Foundation::GetLastError() }
            )
            .into());
        }
        let entry_point =
            unsafe { GetProcAddress(module, c"vkGetInstanceProcAddr".as_ptr().cast()) };
        if entry_point.is_none() {
            unsafe { FreeLibrary(module) };
            return Err(format!(
                "embedded Vulkan loader loaded from {} but vkGetInstanceProcAddr is missing",
                absolute_path.display()
            )
            .into());
        }
        tracing::info!(target: "dogwood_accelerator", "LoadLibraryExW loaded embedded Vulkan loader");
        Ok(Self {
            path: absolute_path,
            module,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LoadedVulkanLoader {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.module) };
    }
}

fn runtime_base_directory() -> (PathBuf, bool) {
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let path = PathBuf::from(local_app_data)
            .join("Dogwood")
            .join("runtime")
            .join("vulkan");
        if fs::create_dir_all(&path).is_ok() {
            return (path, false);
        }
        tracing::warn!(
            target: "dogwood_accelerator",
            path = %path.display(),
            "unable to create LocalAppData Vulkan runtime directory; trying temporary directory"
        );
    } else {
        tracing::warn!(
            target: "dogwood_accelerator",
            "LOCALAPPDATA is unavailable; trying temporary directory for Vulkan runtime"
        );
    }
    (temporary_base_directory(), true)
}

fn temporary_base_directory() -> PathBuf {
    std::env::temp_dir()
        .join("Dogwood")
        .join("runtime")
        .join("vulkan");
}

fn cache_path(base: &Path) -> PathBuf {
    base.join(digest_hex(&LOADER_DIGEST)).join(LOADER_FILENAME)
}

fn materialize(path: &Path) -> Result<bool, Box<dyn Error>> {
    if valid_file(path)? {
        return Ok(true);
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("Vulkan loader cache has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "unable to create Vulkan loader cache directory {}: {error}",
            parent.display()
        )
    })?;
    let temporary = parent.join(format!("{LOADER_FILENAME}.tmp-{}", std::process::id()));
    fs::write(&temporary, LOADER_BYTES).map_err(|error| {
        format!(
            "unable to write embedded Vulkan loader {}: {error}",
            temporary.display()
        )
    })?;
    if !valid_file(&temporary)? {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "embedded Vulkan loader verification failed: {}",
            temporary.display()
        )
        .into());
    }
    if let Err(rename_error) = fs::rename(&temporary, path) {
        if path.exists() {
            fs::remove_file(path).map_err(|error| {
                format!(
                    "unable to replace invalid Vulkan loader {}: {error}",
                    path.display()
                )
            })?;
            fs::rename(&temporary, path).map_err(|error| {
                format!(
                    "unable to install Vulkan loader {}: {error}",
                    path.display()
                )
            })?;
        } else {
            let _ = fs::remove_file(&temporary);
            return Err(format!(
                "unable to install Vulkan loader {}: {rename_error}",
                path.display()
            )
            .into());
        }
    }
    if !valid_file(path)? {
        return Err(format!(
            "installed Vulkan loader verification failed: {}",
            path.display()
        )
        .into());
    }
    Ok(false)
}

fn valid_file(path: &Path) -> Result<bool, Box<dyn Error>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() != LOADER_BYTES.len() as u64 {
        return Ok(false);
    }
    Ok(digest(&fs::read(path)?) == LOADER_DIGEST)
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn digest_hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::{LOADER_BYTES, LOADER_DIGEST, cache_path, digest, materialize};
    use std::{fs, path::Path};

    #[test]
    fn embedded_loader_metadata_matches_bytes() {
        assert_eq!(digest(LOADER_BYTES), LOADER_DIGEST);
        assert!(!LOADER_BYTES.is_empty());
    }

    #[test]
    fn cache_reuses_correct_file_and_replaces_corrupt_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = cache_path(directory.path());
        assert!(!materialize(&path).unwrap());
        assert!(materialize(&path).unwrap());
        fs::write(&path, b"corrupt").unwrap();
        assert!(!materialize(&path).unwrap());
        assert_eq!(fs::read(&path).unwrap(), LOADER_BYTES);
    }

    #[test]
    fn cache_path_is_stable_and_content_specific() {
        let first = cache_path(Path::new("C:/Users/example/AppData/Local"));
        let second = cache_path(Path::new("C:/Users/example/AppData/Local"));
        assert_eq!(first, second);
        assert!(first.ends_with("vulkan-1.dll"));
    }
}
