// Copyright Rob Gage 2026

use std::{env, fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuildTarget {
    WindowsX64,
    WindowsX86,
    WindowsArm64,
    LinuxX64,
    LinuxX86,
    LinuxArm64,
}

impl BuildTarget {
    pub(crate) fn host() -> Result<Self, String> {
        Self::from_host(env::consts::OS, env::consts::ARCH)
    }

    fn from_host(os: &str, arch: &str) -> Result<Self, String> {
        match (os, arch) {
            ("windows", "x86_64") => Ok(Self::WindowsX64),
            ("windows", "x86") => Ok(Self::WindowsX86),
            ("windows", "aarch64") => Ok(Self::WindowsArm64),
            ("linux", "x86_64") => Ok(Self::LinuxX64),
            ("linux", "x86") => Ok(Self::LinuxX86),
            ("linux", "aarch64") => Ok(Self::LinuxArm64),
            _ => Err(format!(
                "unsupported host platform {os}-{arch}; Dogwood supports Linux and Windows x86, x64, and ARM64"
            )),
        }
    }

    pub(crate) fn triple(self) -> &'static str {
        match self {
            Self::WindowsX64 => "x86_64-pc-windows-msvc",
            Self::WindowsX86 => "i686-pc-windows-msvc",
            Self::WindowsArm64 => "aarch64-pc-windows-msvc",
            Self::LinuxX64 => "x86_64-unknown-linux-gnu",
            Self::LinuxX86 => "i686-unknown-linux-gnu",
            Self::LinuxArm64 => "aarch64-unknown-linux-gnu",
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::WindowsX64 => "windows-x64",
            Self::WindowsX86 => "windows-x86",
            Self::WindowsArm64 => "windows-arm64",
            Self::LinuxX64 => "linux-x64",
            Self::LinuxX86 => "linux-x86",
            Self::LinuxArm64 => "linux-arm64",
        }
    }
    pub(crate) fn is_windows(self) -> bool {
        matches!(
            self,
            Self::WindowsX64 | Self::WindowsX86 | Self::WindowsArm64
        )
    }
    pub(crate) fn executable_name(self) -> &'static str {
        if self.is_windows() {
            "game.exe"
        } else {
            "game"
        }
    }

    pub(crate) fn engine_compute_feature(self) -> Option<&'static str> {
        match self {
            Self::WindowsX86 => Some("dogwood_engine_compute/windows-vulkan-loader-x86"),
            Self::WindowsX64 => Some("dogwood_engine_compute/windows-vulkan-loader-x64"),
            Self::WindowsArm64 => Some("dogwood_engine_compute/windows-vulkan-loader-arm64"),
            Self::LinuxX64 | Self::LinuxX86 | Self::LinuxArm64 => None,
        }
    }
}

impl fmt::Display for BuildTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for BuildTarget {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "windows-x64" => Ok(Self::WindowsX64),
            "windows-x86" => Ok(Self::WindowsX86),
            "windows-arm64" => Ok(Self::WindowsArm64),
            "linux-x64" => Ok(Self::LinuxX64),
            "linux-x86" => Ok(Self::LinuxX86),
            "linux-arm64" => Ok(Self::LinuxArm64),
            _ => Err(format!("unsupported target `{value}`")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BuildTarget;

    #[test]
    fn windows_targets_select_one_matching_loader_feature() {
        assert_eq!(
            BuildTarget::WindowsX86.engine_compute_feature(),
            Some("dogwood_engine_compute/windows-vulkan-loader-x86")
        );
        assert_eq!(
            BuildTarget::WindowsX64.engine_compute_feature(),
            Some("dogwood_engine_compute/windows-vulkan-loader-x64")
        );
        assert_eq!(
            BuildTarget::WindowsArm64.engine_compute_feature(),
            Some("dogwood_engine_compute/windows-vulkan-loader-arm64")
        );
    }

    #[test]
    fn linux_targets_do_not_select_a_windows_loader() {
        assert!(BuildTarget::LinuxX64.engine_compute_feature().is_none());
        assert!(BuildTarget::LinuxX86.engine_compute_feature().is_none());
        assert!(BuildTarget::LinuxArm64.engine_compute_feature().is_none());
    }
}
