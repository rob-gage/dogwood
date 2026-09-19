# Embedded Windows Vulkan loader

Dogwood embeds only the official Khronos Vulkan loader DLL. The loader is
materialized at runtime so a Windows game remains one executable; the user's
installed graphics driver still supplies the Vulkan ICD.

## Source and version

These unmodified DLLs come from the LunarG Vulkan Runtime 1.4.357.0 component
archives, which distribute the official Khronos Vulkan loader. The published
archive SHA-256 values are included for provenance:

- x86 and x64: [`VulkanRT-X64-1.4.357.0-Components.zip`](https://sdk.lunarg.com/sdk/download/1.4.357.0/windows/VulkanRT-X64-1.4.357.0-Components.zip) — `a14672efed15aafc7f5a16572d35cd3a3416eadf670aeee3cdf50ee32d5fbf83`
- ARM64: [`VulkanRT-ARM64-1.4.357.0-Components.zip`](https://sdk.lunarg.com/sdk/download/1.4.357.0/warm/VulkanRT-ARM64-1.4.357.0-Components.zip) — `0a51a619525e0c7a156125c4f80c4f591c494cef9ff59dc4481735779a9a280c`

The upstream Vulkan Loader project is maintained by Khronos:
<https://github.com/KhronosGroup/Vulkan-Loader>. The runtime package's
attribution and MIT/Apache license text is in `VulkanRT-License.txt`.

## Embedded files

| Architecture | PE architecture | SHA-256 |
| --- | --- | --- |
| x86 | Intel i386 | `b1ca65b9321ad4e76256fd72f9cb5610ffda1cf2a9a957f07da7be77c8b12272` |
| x64 | x86-64 | `cd862090370454630b31b174e3d4eb474fda38ea034998d1fe1767b0c99a8696` |
| ARM64 | ARM64 | `cc5dd0bec8a7afef013c61ddd511d31500a3b3a45c229150a9ae73ead708ab76` |

Only the architecture selected by the Cargo feature is embedded. To update a
loader, replace the matching DLL, recalculate its SHA-256, update the metadata
in `src/windows_vulkan_loader.rs`, and update this document and the license if
the upstream package changes.
