// Copyright Rob Gage 2026

use std::{
    error::Error,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::ffi::c_void;

const LOADER_NAME: &str = if cfg!(windows) {
    "vulkan-1.dll"
} else {
    "libvulkan.so.1"
};

pub(crate) struct VulkanLoader {
    path: PathBuf,
    #[cfg(windows)]
    module: windows_sys::Win32::Foundation::HMODULE,
    #[cfg(unix)]
    #[allow(dead_code)]
    library: libloading::Library,
}

impl VulkanLoader {
    pub(crate) fn load_packaged() -> Result<Self, Box<dyn Error>> {
        let path = sibling_loader_path(&std::env::current_exe()?)?;
        if !path.is_file() {
            return Err(
                format!("packaged Vulkan loader does not exist: {}", path.display()).into(),
            );
        }

        #[cfg(windows)]
        {
            use std::{ffi::OsStr, os::windows::ffi::OsStrExt};
            use windows_sys::Win32::System::LibraryLoader::{
                GetProcAddress, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
                LoadLibraryExW,
            };
            let wide: Vec<u16> = OsStr::new(&path).encode_wide().chain(Some(0)).collect();
            let module = unsafe {
                LoadLibraryExW(
                    wide.as_ptr(),
                    std::ptr::null_mut(),
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
                )
            };
            if module.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            if unsafe { GetProcAddress(module, c"vkGetInstanceProcAddr".as_ptr() as _) }.is_none() {
                unsafe { windows_sys::Win32::Foundation::FreeLibrary(module) };
                return Err("packaged Vulkan loader does not export vkGetInstanceProcAddr".into());
            }
            Ok(Self { path, module })
        }

        #[cfg(unix)]
        {
            let library = unsafe { libloading::Library::new(&path) }?;
            unsafe { library.get::<*const c_void>(b"vkGetInstanceProcAddr\0") }.map_err(
                |error| format!("packaged Vulkan loader has no vkGetInstanceProcAddr: {error}"),
            )?;
            Ok(Self { path, library })
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(windows)]
impl Drop for VulkanLoader {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::FreeLibrary(self.module) };
    }
}

pub(crate) fn sibling_loader_path(executable: &Path) -> Result<PathBuf, Box<dyn Error>> {
    executable
        .parent()
        .map(|directory| directory.join(LOADER_NAME))
        .ok_or_else(|| {
            format!(
                "executable has no parent directory: {}",
                executable.display()
            )
            .into()
        })
}

#[cfg(test)]
mod tests {
    use super::sibling_loader_path;
    use std::path::Path;

    #[test]
    fn loader_path_uses_executable_directory_not_current_directory() {
        let executable = Path::new("/portable/game/game");
        assert_eq!(
            sibling_loader_path(executable).unwrap(),
            Path::new("/portable/game/libvulkan.so.1")
        );
    }
}
