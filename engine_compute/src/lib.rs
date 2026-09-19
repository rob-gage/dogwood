// Copyright Rob Gage 2026

//! Shared Accelerator resources and composable shader construction.

mod accelerator;
mod accelerator_buffer;
mod accelerator_timing;
#[cfg(debug_assertions)]
mod accelerator_timing_readback;
#[cfg(debug_assertions)]
mod accelerator_timing_record;
#[cfg(debug_assertions)]
mod accelerator_timing_state;
mod shader_composition;
#[cfg(target_os = "windows")]
mod windows_vulkan_loader;

#[cfg(all(
    target_os = "windows",
    not(any(
        feature = "windows-vulkan-loader-x86",
        feature = "windows-vulkan-loader-x64",
        feature = "windows-vulkan-loader-arm64",
    ))
))]
compile_error!("Windows builds require exactly one windows-vulkan-loader-* feature");

#[cfg(all(
    not(target_os = "windows"),
    any(
        feature = "windows-vulkan-loader-x86",
        feature = "windows-vulkan-loader-x64",
        feature = "windows-vulkan-loader-arm64",
    )
))]
compile_error!("windows-vulkan-loader-* features are only valid for Windows targets");

#[cfg(any(
    all(
        feature = "windows-vulkan-loader-x86",
        feature = "windows-vulkan-loader-x64"
    ),
    all(
        feature = "windows-vulkan-loader-x86",
        feature = "windows-vulkan-loader-arm64"
    ),
    all(
        feature = "windows-vulkan-loader-x64",
        feature = "windows-vulkan-loader-arm64"
    ),
))]
compile_error!("enable only one windows-vulkan-loader-* feature");

#[cfg(all(feature = "windows-vulkan-loader-x86", not(target_arch = "x86")))]
compile_error!("windows-vulkan-loader-x86 requires an i686 Windows target");
#[cfg(all(feature = "windows-vulkan-loader-x64", not(target_arch = "x86_64")))]
compile_error!("windows-vulkan-loader-x64 requires an x86_64 Windows target");
#[cfg(all(feature = "windows-vulkan-loader-arm64", not(target_arch = "aarch64")))]
compile_error!("windows-vulkan-loader-arm64 requires an aarch64 Windows target");

pub use accelerator::Accelerator;
pub use accelerator_buffer::AcceleratorBuffer;
pub use shader_composition::{
    ComposableShaderUtility, create_composed_shader_module,
    create_composed_shader_module_with_utilities,
};
