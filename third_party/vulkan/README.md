# Windows Vulkan fallback loaders

Official LunarG runtime components, not GPU drivers. The binary files, licenses and
`manifest.json` are checked into this repository and embedded in the Dogwood CLI.

Refresh both architectures to the newest published public runtime component ZIPs:

```sh
python3 tools/update_vulkan_loaders.py
python3 tools/update_vulkan_loaders.py --check
cargo install --path cli --locked
```

The Python 3 standard-library updater reads https://vulkan.lunarg.com/sdk/files.json,
selects the latest runtime archive separately for `windows` (x64) and `warm` (ARM64),
verifies LunarG's published SHA-256, and checks the DLL's PE machine before extracting
only `vulkan-1.dll` and `VulkanRT-License.txt`. Both archives are validated before
vendored files are changed. The manifest records URLs, versions, archive/DLL/license
hashes and the update time. `--cache-dir DIR` reuses verified ZIP downloads.
SDK-only patch releases do not necessarily publish a newer runtime loader.

Distribution layout: `<executable-directory>/vulkan/vulkan-1.dll`, with the runtime
license and provenance manifest alongside it. The loader is deliberately absent
from the executable directory itself. Before wgpu initialization, Dogwood explicitly
loads the system `vulkan-1.dll` using `LOAD_LIBRARY_SEARCH_SYSTEM32`. Only when that
fails does it load the fallback by absolute executable-relative path. The chosen
module is retained so wgpu/ash's later name-based load reuses it. Current-working-
directory DLLs are never used to select the Vulkan loader. A present but unusable
GPU/driver does not trigger a switch from an already loaded system loader.
