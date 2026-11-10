// Copyright Rob Gage 2026

//! Preload the system Vulkan loader before wgpu/ash opens it by module name.
//! The selected module stays loaded for the process lifetime.

#[cfg(any(windows, test))]
fn system_or_fallback<T>(
    system: impl FnOnce() -> Result<T, String>,
    fallback: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    match system() {
        Ok(module) => Ok(module),
        Err(system_error) => fallback().map_err(|fallback_error| {
            format!("System Vulkan loader: {system_error}; bundled fallback: {fallback_error}")
        }),
    }
}

#[cfg(windows)]
pub(super) fn ensure_loaded() -> Result<(), String> {
    use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path, sync::OnceLock};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> Option<unsafe extern "system" fn() -> isize>;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }

    fn load(path: &Path, flags: u32) -> Result<usize, String> {
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: name is NUL-terminated; flags restrict dependent DLL searches to
        // the selected DLL's directory and/or System32, never the working directory.
        let module = unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), flags) };
        if module.is_null() {
            return Err(format!("{}: {}", path.display(), std::io::Error::last_os_error()));
        }
        // SAFETY: module is a live Windows module and the export name is NUL-terminated.
        if unsafe { GetProcAddress(module, c"vkGetInstanceProcAddr".as_ptr().cast()) }.is_none() {
            // SAFETY: this attempt owns a module reference that has not escaped.
            unsafe { FreeLibrary(module); }
            return Err(format!("{} lacks vkGetInstanceProcAddr", path.display()));
        }
        // Retain this reference: ash subsequently opens vulkan-1.dll by name and
        // Windows reuses the already loaded module instead of searching beside the exe.
        Ok(module as usize)
    }

    static MODULE: OnceLock<Result<usize, String>> = OnceLock::new();
    MODULE.get_or_init(|| {
        system_or_fallback(
            || load(Path::new("vulkan-1.dll"), 0x0000_0800), // LOAD_LIBRARY_SEARCH_SYSTEM32
            || {
                let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                let directory = executable.parent().ok_or("executable has no parent directory")?;
                let path = directory.join("vulkan/vulkan-1.dll");
                // LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32
                let module = load(&path, 0x0000_0900)?;
                eprintln!("Dogwood Vulkan: using bundled fallback {}", path.display());
                Ok(module)
            },
        )
    }).as_ref().map(|_| ()).map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_loader_wins_without_touching_fallback() {
        assert_eq!(system_or_fallback(|| Ok(1), || panic!("fallback must not load")), Ok(1));
    }

    #[test]
    fn missing_system_loader_uses_fallback_and_reports_both_failures() {
        assert_eq!(system_or_fallback(|| Err("missing".into()), || Ok(2)), Ok(2));
        let error = system_or_fallback::<()>(|| Err("system missing".into()), || Err("fallback missing".into())).unwrap_err();
        assert!(error.contains("system missing") && error.contains("fallback missing"));
    }
}
