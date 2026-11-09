//! Optional process-local hints; no machine-wide power or scheduling changes.
pub(super) fn configure() {
    if std::env::var("DOGWOOD_PROCESS_PRIORITY").as_deref() != Ok("above-normal") {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn SetPriorityClass(process: *mut std::ffi::c_void, class: u32) -> i32;
        }
        // ABOVE_NORMAL_PRIORITY_CLASS is conventional and never realtime.
        if unsafe { SetPriorityClass(GetCurrentProcess(), 0x00008000) } == 0 {
            eprintln!(
                "Dogwood: process priority hint unavailable: {}",
                std::io::Error::last_os_error()
            );
        }
    }
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn setpriority(which: u32, who: u32, priority: i32) -> i32;
        }
        if unsafe { setpriority(0, 0, -5) } != 0 {
            eprintln!(
                "Dogwood: process priority hint unavailable; continuing normally: {}",
                std::io::Error::last_os_error()
            );
        }
    }
}
